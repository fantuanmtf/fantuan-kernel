//! kernel-riscv — M9.1/M9.2: RISC-V bring-up (DESIGN.md §14,
//! docs/M9_KERNEL_v0.0.1.md).
//!
//! OpenSBI (QEMU `-bios default`) enters `_start` in S-mode with a0 = hartid,
//! a1 = DTB (verified by spike). This kernel parses the FDT memory map,
//! synthesizes a BootInfo for the shared kernel-core frame allocator, builds
//! Sv39 tables (identity + PHYS_OFFSET alias) and enters the high half.

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::mem::size_of;
use core::panic::PanicInfo;

use fantuan_abi::{
    BootInfo, FrameBuffer, MemMap, MemoryDescriptor, BOOT_MAGIC, BOOT_VERSION,
    MEMORY_TYPE_CONVENTIONAL,
};

mod cpu;
mod uart;
pub(crate) use uart::{park, put_bytes, put_dec, put_hex, puts, uart_putc};
mod demo;
mod fdt;
mod paging;
mod sbi;
mod syscall;
mod task;
mod timer;
mod trap;

include!(concat!(env!("OUT_DIR"), "/user_program.rs"));

const UART_BASE: usize = uart::UART_BASE;
/// QEMU virt RAM base and the OpenSBI kernel load address (link.ld).
const RAM_BASE: u64 = 0x8000_0000;
const KERNEL_BASE: u64 = 0x8020_0000;

extern "C" {
    static __bss_end: u8;
    /// Top of the assembly boot stack (global_asm in this file).
    static BOOT_STACK_TOP: u8;
}

global_asm!(
    ".section .text.entry",
    ".global _start",
    ".type _start, @function",
    "_start:",
    // OpenSBI does not define sp for the payload: claim our own stack first.
    "    la      sp, BOOT_STACK_TOP",
    // Zero .bss (includes the stack region while it is still empty).
    "    la      t0, __bss_start",
    "    la      t1, __bss_end",
    "1:",
    "    bgeu    t0, t1, 2f",
    "    sd      zero, 0(t0)",
    "    addi    t0, t0, 8",
    "    j       1b",
    "2:",
    // a0 = hartid, a1 = DTB survive the loop (t-registers only).
    "    call    rust_entry",
    "3:",
    "    wfi",
    "    j       3b",
    ".section .bss",
    ".align 4",
    ".global BOOT_STACK",
    "BOOT_STACK:",
    "    .space 16384",
    ".global BOOT_STACK_TOP",
    "BOOT_STACK_TOP:",
);

// --- BootInfo synthesis from the FDT ---------------------------------------

const EMPTY_DESC: MemoryDescriptor = MemoryDescriptor {
    type_: 0,
    physical_start: 0,
    virtual_start: 0,
    number_of_pages: 0,
    attribute: 0,
};

static mut MEMMAP: [MemoryDescriptor; 16] = [EMPTY_DESC; 16];
/// FDT timebase, stashed for the high-half boot (the timer arms there).
static mut TIMEBASE_HZ: u64 = 10_000_000;
static mut BOOT_INFO: BootInfo = BootInfo {
    magic: BOOT_MAGIC,
    version: BOOT_VERSION,
    memmap: MemMap { ptr: core::ptr::null(), count: 0, desc_size: size_of::<MemoryDescriptor>() },
    fb: FrameBuffer { base: 0, size: 0, width: 0, height: 0, stride: 0, format: 0 },
    rsdp: 0,
    kernel_base: KERNEL_BASE,
    stack_top: 0,
    caps: 0,
    boot_pml4: 0,
    boot_tables_pages: 0,
    runtime_services: 0,
    smbios_table: 0,
    arch: 2, // riscv64 OpenSBI
    hartid: 0,
    dtb: 0,
};

fn build_bootinfo(mem: &fdt::MemInfo, hartid: usize, dtb: usize) -> &'static BootInfo {
    unsafe {
        let map = &mut *core::ptr::addr_of_mut!(MEMMAP);
        let mut n = 0usize;
        for i in 0..mem.mem_n {
            if n >= map.len() {
                break;
            }
            map[n] = MemoryDescriptor {
                type_: MEMORY_TYPE_CONVENTIONAL,
                physical_start: mem.mem[i].base,
                virtual_start: 0,
                number_of_pages: mem.mem[i].size / 4096,
                attribute: 0,
            };
            n += 1;
        }
        let bi = &mut *core::ptr::addr_of_mut!(BOOT_INFO);
        bi.memmap.ptr = map.as_ptr();
        bi.memmap.count = n;
        bi.stack_top = core::ptr::addr_of!(BOOT_STACK_TOP) as u64;
        bi.hartid = hartid as u64;
        bi.dtb = dtb as u64;
        &*core::ptr::addr_of!(BOOT_INFO)
    }
}

// --- Boot ------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn rust_entry(hartid: usize, dtb: usize) -> ! {
    puts("fantuan (riscv64) M9.3 - OpenSBI S-mode bring-up\n");
    puts("boot: hartid=");
    put_hex(hartid as u64);
    puts(" dtb=");
    put_hex(dtb as u64);
    puts("\n");

    let Some(mem) = fdt::parse(dtb) else {
        puts("fdt: parse failed - first word=");
        put_hex(unsafe { core::ptr::read_volatile(dtb as *const u32) } as u64);
        puts(" - parking\n");
        park();
    };
    for i in 0..mem.mem_n {
        puts("fdt: memory ");
        put_hex(mem.mem[i].base);
        puts("..");
        put_hex(mem.mem[i].base + mem.mem[i].size);
        puts("\n");
    }
    puts("cpu: ");
    put_dec(mem.harts as u64);
    puts(" hart(s), isa=");
    put_bytes(&mem.isa[..mem.isa_len]);
    puts(", model=");
    put_bytes(&mem.model[..mem.model_len]);
    puts("\n");

    // Shared allocator (kernel-core): install the IRQ hooks first.
    kernel_core::arch::set_irq_ops(cpu::irq_save, cpu::irq_restore);
    let bi = build_bootinfo(&mem, hartid, dtb);

    // Reservations the FDT memory map does not express: the firmware region
    // below the kernel, the memreserve block and the DTB itself.
    let mut extra = [(0u64, 0u64); 12];
    let mut n = 0usize;
    extra[n] = (RAM_BASE, KERNEL_BASE);
    n += 1;
    for i in 0..mem.reserved_n {
        if n < extra.len() {
            extra[n] = (mem.reserved[i].base, mem.reserved[i].base + mem.reserved[i].size);
            n += 1;
        }
    }
    extra[n] = (dtb as u64, dtb as u64 + mem.totalsize as u64);
    n += 1;

    unsafe { TIMEBASE_HZ = mem.timebase };
    let kernel_end = core::ptr::addr_of!(__bss_end) as u64;
    kernel_core::frame::init(bi, kernel_end, &extra[..n]);
    puts("mm: usable ");
    put_dec(kernel_core::frame::get().usable_mib());
    puts(" MiB\n");

    // Sv39: identity + alias for RAM (2 MiB leaves) and the UART (4 KiB).
    paging::init();
    for i in 0..mem.mem_n {
        let b = mem.mem[i];
        let mut pa = b.base & !(2 * 1024 * 1024 - 1);
        let end = b.base + b.size;
        while pa < end {
            paging::map_2m(pa, pa, paging::ram_flags());
            paging::map_2m(paging::phys_to_virt(pa), pa, paging::ram_flags());
            pa += 2 * 1024 * 1024;
        }
    }
    paging::map_4k(UART_BASE as u64, UART_BASE as u64, paging::mmio_flags());
    paging::map_4k(
        paging::phys_to_virt(UART_BASE as u64),
        UART_BASE as u64,
        paging::mmio_flags(),
    );
    puts("paging: Sv39 tables built; entering the high half\n");
    paging::enable();

    // Move the stack and the program counter into the alias.
    let sp: usize;
    unsafe { asm!("mv {}, sp", out(reg) sp, options(nomem, nostack)) };
    unsafe { asm!("mv sp, {}", in(reg) paging::phys_to_virt(sp as u64), options(nostack)) };
    unsafe { paging::riscv_jump(paging::phys_to_virt(high_main as *const () as u64) as usize) }
}

extern "C" fn high_main() -> ! {
    paging::use_alias();
    puts("fantuan (riscv64) M9.3 - high half online\n");
    puts("mm: usable ");
    put_dec(kernel_core::frame::get().usable_mib());
    puts(" MiB\n");

    match kernel_core::frame::get().alloc() {
        Some(f) => {
            let p = paging::phys_to_virt(f) as *mut u64;
            unsafe { p.write_volatile(0xF0F0_F0F0_DEAD_BEEF) };
            let ok = unsafe { p.read_volatile() } == 0xF0F0_F0F0_DEAD_BEEF;
            kernel_core::frame::get().free(f);
            puts(if ok {
                "mm: frame self-test ok (via PHYS_OFFSET alias)\n"
            } else {
                "mm: frame self-test FAILED\n"
            });
        }
        None => puts("mm: frame self-test FAILED (no frames)\n"),
    }
    puts("paging: Sv39 identity + PHYS_OFFSET alias active\n");

    // M9.2b: traps + SBI timer.
    trap::init();

    puts("trap: stvec armed\n");
    unsafe { asm!("ebreak") };
    puts("trap: resumed after ebreak\n");
    timer::init(unsafe { TIMEBASE_HZ });
    puts("timer: SBI timer armed at 100 Hz\n");

    // M9.2c: shared scheduler + two kernel demo tasks.
    task::init_arch();
    let boot_stack_top = paging::phys_to_virt(core::ptr::addr_of!(BOOT_STACK_TOP) as u64);
    kernel_core::task::init(boot_stack_top);
    kernel_core::task::spawn(demo::demo_1);
    kernel_core::task::spawn(demo::demo_2);
    puts("sched: 2 riscv kernel tasks spawned\n");

    // M9.3c: two userland tasks from the embedded ELF.
    let mut spawned = 0;
    for _ in 0..2 {
        if task::spawn_user(USER_ELF).is_some() {
            spawned += 1;
        }
    }
    puts("user: ");
    put_dec(spawned);
    puts(" riscv user tasks spawned\n");
    park()
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    puts("PANIC: kernel-riscv\n");
    park()
}
