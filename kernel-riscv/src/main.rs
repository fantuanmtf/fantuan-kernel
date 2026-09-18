//! kernel-riscv — M9.1a: RISC-V bring-up (DESIGN.md §14,
//! docs/M9_KERNEL_v0.0.1.md).
//!
//! OpenSBI (QEMU `-bios default`) enters `_start` in S-mode with a0 = hartid,
//! a1 = DTB (verified by spike). This milestone prints the handoff over the
//! NS16550 MMIO UART; Sv39 paging, FDT-lite and the frame allocator follow in
//! M9.1b.

#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;

/// QEMU virt NS16550 UART (DESIGN §14.1, spike-verified).
const UART_BASE: usize = 0x1000_0000;

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

fn put_hex(mut v: usize) {
    const HEX: &[u8] = b"0123456789abcdef";
    puts("0x");
    let mut buf = [0u8; 16];
    let mut n = 0;
    if v == 0 {
        uart_putc(b'0');
        return;
    }
    while v > 0 {
        buf[n] = HEX[v & 0xF];
        v >>= 4;
        n += 1;
    }
    while n > 0 {
        n -= 1;
        uart_putc(buf[n]);
    }
}

#[no_mangle]
pub extern "C" fn rust_entry(hartid: usize, dtb: usize) -> ! {
    puts("fantuan (riscv64) M9.1 - OpenSBI S-mode bring-up\n");
    puts("boot: hartid=");
    put_hex(hartid);
    puts(" dtb=");
    put_hex(dtb);
    puts("\n");
    puts("mm: Sv39 paging + frame allocator arrive in M9.1b\n");
    loop {
        unsafe { core::arch::asm!("wfi") }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    puts("PANIC: kernel-riscv\n");
    loop {
        unsafe { core::arch::asm!("wfi") }
    }
}
