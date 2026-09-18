//! NS16550 MMIO UART helpers (QEMU virt, 0x10000000) and the park loop.
//! Split out of main.rs to keep every file inside the size rule.

use core::arch::asm;

/// QEMU virt NS16550 UART (DESIGN §14.1, spike-verified).
pub const UART_BASE: usize = 0x1000_0000;

pub fn uart_putc(c: u8) {
    // Follow the paging access base: identity while bare, the alias once the
    // high half is online. User roots share the alias but not the identity.
    let base = UART_BASE as u64 + crate::paging::access_base();
    unsafe { core::ptr::write_volatile(base as *mut u8, c) }
}

/// Boot splash: ASCII logo plus the release signature line.
pub const LOGO: &str = concat!(
    "  __                _\n",
    " / _| __ _ _ __  | |_ _   _  __ _ _ __\n",
    "| |_ / _` | '  \\| __| | | |/ _` | '  \\\n",
    "|  _| (_| | | | | |_| |_| | (_| | | | |\n",
    "|_|  \\__,_|_| |_|\\__|\\__,_|\\__,_|_| |_|\n",
    "  fantuan v0.0.1 - fantuan-is-mtf\n",
);

/// Non-blocking byte read (LSR bit 0 = data ready, RBR at offset 0).
pub fn getc() -> Option<u8> {
    let base = UART_BASE as u64 + crate::paging::access_base();
    let ready = unsafe { core::ptr::read_volatile((base + 5) as *const u8) } & 1;
    if ready == 0 {
        return None;
    }
    Some(unsafe { core::ptr::read_volatile(base as *const u8) })
}

/// Byte sink for the shared logger (LF -> CRLF, like puts).
pub fn log_bytes(buf: &[u8]) {
    for &b in buf {
        if b == b'\n' {
            uart_putc(b'\r');
        }
        uart_putc(b);
    }
}

pub fn puts(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            uart_putc(b'\r');
        }
        uart_putc(b);
    }
}

/// Print raw bytes (FDT strings).
pub fn put_bytes(s: &[u8]) {
    for &b in s {
        uart_putc(b);
    }
}

pub fn put_hex(mut v: u64) {
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

pub fn put_dec(mut v: u64) {
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

/// Wait for the next interrupt forever (idle loop).
pub fn park() -> ! {
    loop {
        unsafe { asm!("wfi") }
    }
}
