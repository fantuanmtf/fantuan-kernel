//! 16550 COM1 output for the i686 kernel (port I/O, no formatting machinery
//! beyond what the shared log sink needs).

use crate::cpu::{inb, outb};

pub const COM1: u16 = 0x3F8;

/// Optional framebuffer mirror (M10-5); set once the console is up. Every
/// byte written to COM1 is also handed to the sink, so the two channels stay
/// in lockstep and the serial output is unchanged when no framebuffer exists.
static mut MIRROR: Option<fn(u8)> = None;

pub fn set_mirror(f: fn(u8)) {
    unsafe { core::ptr::addr_of_mut!(MIRROR).write(Some(f)) };
}

pub fn init() {
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x80);
    outb(COM1 + 0, 1);
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x03);
    outb(COM1 + 2, 0xC7);
    outb(COM1 + 4, 0x0B);
}

pub fn putc(c: u8) {
    while inb(COM1 + 5) & 0x20 == 0 {}
    outb(COM1, c);
    let m = unsafe { core::ptr::addr_of!(MIRROR).read() };
    if let Some(f) = m {
        f(c);
    }
}

pub fn puts(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            putc(b'\r');
        }
        putc(b);
    }
}

/// Byte sink for `kernel-core::log` (expands LF to CRLF).
pub fn log_bytes(buf: &[u8]) {
    for &b in buf {
        if b == b'\n' {
            putc(b'\r');
        }
        putc(b);
    }
}

pub fn put_hex(mut v: u64) {
    puts("0x");
    if v == 0 {
        putc(b'0');
        return;
    }
    const HEX: &[u8] = b"0123456789abcdef";
    let mut buf = [0u8; 16];
    let mut n = 0;
    while v > 0 && n < buf.len() {
        buf[n] = HEX[(v & 0xF) as usize];
        v >>= 4;
        n += 1;
    }
    while n > 0 {
        n -= 1;
        putc(buf[n]);
    }
}

pub fn put_dec(mut v: u64) {
    if v == 0 {
        putc(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut n = 0;
    while v > 0 {
        buf[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    while n > 0 {
        n -= 1;
        putc(buf[n]);
    }
}
