//! kernel-aarch64 — M11 R9a: aarch64 direct-FDT bring-up on QEMU `virt`.
//!
//! QEMU loads the ELF at its physical link address and enters `_start` at
//! EL1 (EL2 is dropped first when the machine exposes it) with x0 = physical
//! DTB. This kernel parses the FDT memory map, synthesizes a BootInfo for the
//! shared frame allocator, builds 4K-granule stage-1 tables (identity for
//! RAM/UART/GIC plus a TTBR1 direct map at PHYS_OFFSET), brings up GICv2 and
//! the EL1 physical timer at 100 Hz, then runs the shared scheduler and
//! shell. Storage/user mode/network are R9b work.

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

mod boot;
mod cpu;
mod demo;
mod drivers;
mod fdt;
mod gic;
mod mmu;
#[cfg(kconfig_net)]
mod net;
mod shell;
mod task;
mod timer;
mod trap;
mod uart;

pub(crate) use uart::{park, put_bytes, put_dec, put_hex, puts, uart_putc};

extern "C" {
    static __bss_end: u8;
    static BOOT_STACK_TOP: u8;
}

global_asm!(
    ".section .text.entry",
    ".global _start",
    ".type _start, @function",
    "_start:",
    // QEMU enters at EL2 when the machine exposes it and EL1 otherwise
    // (QEMU 10 virt with GICv2 enters EL1; the drop keeps EL2 machines safe).
    "    mrs x1, CurrentEL",
    "    cmp x1, #(2 << 2)",
    "    b.eq 3f",
    "    b 4f",
    "3:",
    "    mov x1, #3", // CNTHCTL_EL2: EL1PCEN | EL1PCTEN
    "    msr cnthctl_el2, x1",
    "    mov x1, #(1 << 31)", // HCR_EL2.RW = 1 (EL1 is AArch64)
    "    msr hcr_el2, x1",
    "    mov x1, #0x3C5", // SPSR_EL2: EL1h, DAIF masked
    "    msr spsr_el2, x1",
    "    adr x1, 4f",
    "    msr elr_el2, x1",
    "    eret",
    "4:",
    "    mov x19, x0", // x0 = DTB survives until rust_entry
    "    msr spsel, #1",
    "    ldr x1, =BOOT_STACK_TOP",
    "    mov sp, x1",
    "    ldr x1, =__bss_start",
    "    ldr x2, =__bss_end",
    "5:",
    "    cmp x1, x2",
    "    b.hs 6f",
    "    str xzr, [x1], #8",
    "    b 5b",
    "6:",
    "    mov x0, x19",
    "    bl rust_entry",
    "7:",
    "    wfi",
    "    b 7b",
    ".section .bss",
    ".align 4",
    ".global BOOT_STACK",
    "BOOT_STACK:",
    "    .space 16384",
    ".global BOOT_STACK_TOP",
    "BOOT_STACK_TOP:",
);

#[no_mangle]
pub extern "C" fn rust_entry(dtb: usize) -> ! {
    cpu::init();
    uart::init();
    kernel_core::log::set_sink(uart::log_bytes);
    puts(uart::LOGO);
    puts("fantuan v0.0.3 (aarch64) - QEMU virt\n");
    // The boot protocol (QEMU raw `Image` path) passes the DTB in x0; the
    // ELF path does not, so a missing pointer is a usage error, not a fault.
    if dtb == 0 {
        puts("boot: x0 is not a DTB - boot build/kernel-aarch64.bin (raw image), not the ELF\n");
        park();
    }
    puts("boot: EL1, dtb=");
    put_hex(dtb as u64);
    puts("\n");
    puts("uart: pl011 up\n");

    let Some(mem) = fdt::parse(dtb) else {
        puts("fdt: parse failed - first word=");
        put_hex(unsafe { core::ptr::read_volatile(dtb as *const u32) } as u64);
        puts(" - parking\n");
        park();
    };
    fdt::print_info(&mem);

    // Shared allocator (kernel-core): install the IRQ/idle/alias hooks first.
    kernel_core::arch::set_irq_ops(cpu::irq_save, cpu::irq_restore);
    kernel_core::arch::set_irq_enable(cpu::irq_enable);
    kernel_core::arch::set_idle(cpu::idle);
    kernel_core::mem::set_phys_to_virt(mmu::phys_to_virt);
    let stack_top = core::ptr::addr_of!(BOOT_STACK_TOP) as u64;
    let bi = boot::build(&mem, dtb, stack_top);

    // Reservations the FDT memory map does not express: the firmware hole
    // below the kernel image, the memreserve block and the DTB itself.
    let mut extra = [(0u64, 0u64); 12];
    let mut n = 0usize;
    extra[n] = (boot::RAM_BASE, boot::KERNEL_BASE);
    n += 1;
    for i in 0..mem.reserved_n {
        if n < extra.len() {
            extra[n] = (mem.reserved[i].base, mem.reserved[i].base + mem.reserved[i].size);
            n += 1;
        }
    }
    extra[n] = (dtb as u64, dtb as u64 + mem.totalsize as u64);
    n += 1;

    let kernel_end = core::ptr::addr_of!(__bss_end) as u64;
    kernel_core::frame::init(bi, kernel_end, &extra[..n]);
    puts("mm: frame allocator ready: ");
    put_dec(kernel_core::frame::get().usable_mib());
    puts(" MiB usable\n");

    // 4K granule: RAM identity + direct map (2 MiB leaves), UART/GIC device.
    mmu::init();
    for i in 0..mem.mem_n {
        let b = mem.mem[i];
        let mut pa = b.base & !(2 * 1024 * 1024 - 1);
        let end = b.base + b.size;
        while pa < end {
            mmu::map_ram(pa);
            pa += 2 * 1024 * 1024;
        }
    }
    mmu::map_mmio(uart::UART_BASE as u64);
    mmu::map_mmio(0x0800_0000); // GICv2 distributor + CPU interface
    mmu::enable();
    mmu::use_alias();
    puts("mmu: 4K granule, direct map at ");
    put_hex(mmu::PHYS_OFFSET);
    puts("\n");

    match kernel_core::frame::get().alloc() {
        Some(f) => {
            let p = mmu::phys_to_virt(f) as *mut u64;
            unsafe { p.write_volatile(0xF0F0_F0F0_DEAD_BEEF) };
            let ok = unsafe { p.read_volatile() } == 0xF0F0_F0F0_DEAD_BEEF;
            kernel_core::frame::get().free(f);
            puts(if ok {
                "mm: frame self-test ok (via direct map)\n"
            } else {
                "mm: frame self-test FAILED\n"
            });
        }
        None => puts("mm: frame self-test FAILED (no frames)\n"),
    }

    gic::init();
    puts("intc: GICv2 up\n");
    trap::init();

    task::init_arch();
    kernel_core::task::init(stack_top);
    let frq = timer::init();
    puts("timer: 100 Hz (cntfrq ");
    put_dec(frq);
    puts(", PPI 30)\n");

    // Deliberate exception demo: BRK from EL1 must resume after the insn.
    unsafe { asm!("brk #0", options(nomem, nostack)) };
    puts("exc: resumed after brk\n");

    kernel_core::task::spawn(demo::demo_1);
    kernel_core::task::spawn(demo::demo_2);
    puts("sched: 2 aarch64 kernel tasks spawned\n");

    // M11 R9b: NetBSD rump adaptation layer (pools, callouts, mbufs) and
    // the polled virtio-net MMIO driver (CONFIG_NET).
    #[cfg(kconfig_net)]
    net::init();

    shell::enter(bi)
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    puts("PANIC: kernel-aarch64\n");
    park()
}
