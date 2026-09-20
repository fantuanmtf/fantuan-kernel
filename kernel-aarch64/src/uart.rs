//! PL011 UART helpers (QEMU virt, 0x0900_0000) and the park loop. The
//! identity map keeps this physical address valid before and after the MMU
//! comes up, so no access-base switch is needed here.

use core::arch::asm;

/// QEMU virt PL011 (hardcoded; the FDT scan only sanity-checks it).
pub const UART_BASE: usize = 0x0900_0000;

const DR: usize = 0x00;
const FR: usize = 0x18;
const IBRD: usize = 0x24;
const FBRD: usize = 0x28;
const LCR_H: usize = 0x2C;
const CR: usize = 0x30;

fn rd(off: usize) -> u32 {
    unsafe { core::ptr::read_volatile((UART_BASE + off) as *const u32) }
}

fn wr(off: usize, v: u32) {
    unsafe { core::ptr::write_volatile((UART_BASE + off) as *mut u32, v) }
}

/// 8N1, FIFOs on, TX/RX enabled. QEMU ignores the divider; the values are
/// the real 115200 @ 24 MHz ones so the code stays correct off QEMU.
pub fn init() {
    wr(CR, 0);
    wr(IBRD, 13);
    wr(FBRD, 1);
    wr(LCR_H, 0x70); // WLEN=8, FIFO enable
    wr(CR, 1 | (1 << 8) | (1 << 9)); // UARTEN | TXE | RXE
}

pub fn uart_putc(c: u8) {
    while rd(FR) & (1 << 5) != 0 {} // TXFF
    wr(DR, c as u32);
}

/// Boot splash: ASCII logo plus the release signature line.
pub const LOGO: &str = concat!(
    "  __                _\n",
    " / _| __ _ _ __  | |_ _   _  __ _ _ __\n",
    "| |_ / _` | '  \\| __| | | |/ _` | '  \\\n",
    "|  _| (_| | | | | |_| |_| | (_| | | | |\n",
    "|_|  \\__,_|_| |_|\\__|\\__,_|\\__,_|_| |_|\n",
    "  fantuan v0.0.2 - fantuan-is-mtf\n",
);

/// Non-blocking byte read (FR bit 4 = RXFE).
pub fn getc() -> Option<u8> {
    if rd(FR) & (1 << 4) != 0 {
        return None;
    }
    Some(rd(DR) as u8)
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
