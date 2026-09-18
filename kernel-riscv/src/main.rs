//! kernel-riscv — M9.1: RISC-V bring-up (DESIGN.md §14,
//! docs/M9_KERNEL_v0.0.1.md).
//!
//! OpenSBI (QEMU `-bios default`) enters `_start` in S-mode with a0 = hartid,
//! a1 = DTB (verified by spike). M9.1a prints the handoff over the NS16550
//! MMIO UART; M9.1b parses the FDT memory map, initializes the frame
//! allocator, builds Sv39 tables (identity + PHYS_OFFSET alias) and enters
//! the high half.

#![no_std]
#![no_main]

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

mod fdt;
mod frame;
mod paging;

/// QEMU virt NS16550 UART (DESIGN §14.1, spike-verified).
const UART_BASE: usize = 0x1000_0000;
/// QEMU virt RAM base; firmware and the kernel image live at its start.
const RAM_BASE: u64 = 0x8000_0000;

extern "C" {
    static __bss_end: u8;
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

fn uart_putc(c: u8) {
    unsafe { core::ptr::write_volatile(UART_BASE as *mut u8, c) }
}

fn puts(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            uart_putc(b'\r');
        }
        uart_putc(b);
    }
}

fn put_hex(mut v: u64) {
    const HEX: &[u8] = b"0123456789abcdef";
    puts("0x");
    let mut buf = [0u8; 16];
    let mut n = 0;
    if v == 0 {
        uart_putc(b'0');
        return;
    }
    while v > 0 {
        buf[n] = HEX[(v & 0xF) as usize];
        v >>= 4;
        n += 1;
    }
    while n > 0 {
        n -= 1;
        uart_putc(buf[n]);
    }
}

fn put_dec(mut v: u64) {
    let mut buf = [0u8; 20];
    let mut n = 0;
    if v == 0 {
        uart_putc(b'0');
        return;
    }
    while v > 0 {
        buf[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    while n > 0 {
        n -= 1;
        uart_putc(buf[n]);
    }
}

fn park() -> ! {
    loop {
        unsafe { asm!("wfi") }
    }
}

#[no_mangle]
pub extern "C" fn rust_entry(hartid: usize, dtb: usize) -> ! {
    puts("fantuan (riscv64) M9.1 - OpenSBI S-mode bring-up\n");
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
    for i in 0..mem.reserved_n {
        puts("fdt: reserved ");
        put_hex(mem.reserved[i].base);
        puts("..");
        put_hex(mem.reserved[i].base + mem.reserved[i].size);
        puts("\n");
    }

    // Frame allocator: free the FDT memory ranges, keep firmware/kernel/DTB.
    frame::init(&mem);
    let kernel_end = core::ptr::addr_of!(__bss_end) as u64;
    frame::reserve(RAM_BASE, kernel_end);
    frame::reserve(dtb as u64, dtb as u64 + mem.totalsize as u64);
    puts("mm: usable ");
    put_dec(frame::get().usable_mib());
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
    puts("fantuan (riscv64) M9.1b - high half online\n");
    puts("mm: usable ");
    put_dec(frame::get().usable_mib());
    puts(" MiB\n");

    match frame::get().alloc() {
        Some(f) => {
            let p = paging::phys_to_virt(f) as *mut u64;
            unsafe { p.write_volatile(0xF0F0_F0F0_DEAD_BEEF) };
            let ok = unsafe { p.read_volatile() } == 0xF0F0_F0F0_DEAD_BEEF;
            frame::get().free(f);
            puts(if ok {
                "mm: frame self-test ok (via PHYS_OFFSET alias)\n"
            } else {
                "mm: frame self-test FAILED\n"
            });
        }
        None => puts("mm: frame self-test FAILED (no frames)\n"),
    }
    puts("paging: Sv39 identity + PHYS_OFFSET alias active\n");
    park()
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    puts("PANIC: kernel-riscv\n");
    park()
}
