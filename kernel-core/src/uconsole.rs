//! Console formatting for the boot ports: the byte sink each UART driver
//! installs, and the `puts`/`put_hex`/`put_dec` helpers built on it.
//!
//! Merged in 2026-10. `kernel-riscv/src/uart.rs` and
//! `kernel-aarch64/src/uart.rs` carried byte-identical copies of everything
//! below, `LOGO` string and all. The *drivers* stay per-arch and should:
//! NS16550 is 8-bit registers at a fixed offset behind a paging access-base
//! switch, the PL011 is 32-bit registers with its own init sequence, and
//! neither abstraction fits the other. The formatting above them had no such
//! excuse.
//!
//! The sink is a plain fn pointer installed once at boot, not a trait object:
//! there is exactly one console. `install` must be the first statement of the
//! arch entry, before anything can print — miss it and the boot log is simply
//! empty, which the smoke suites catch on their first assertion.

use core::sync::atomic::{AtomicUsize, Ordering};

static PUTC: AtomicUsize = AtomicUsize::new(0);

/// Install the byte sink (called once, at the top of the arch entry).
pub fn set_putc(f: fn(u8)) {
    PUTC.store(f as usize, Ordering::Release);
}

/// Write one byte, or drop it when no sink is installed yet.
pub fn putc(c: u8) {
    let p = PUTC.load(Ordering::Acquire);
    if p == 0 {
        return;
    }
    let f: fn(u8) = unsafe { core::mem::transmute(p) };
    f(c);
}

/// Byte sink for the shared logger (LF -> CRLF, like `puts`).
pub fn log_bytes(buf: &[u8]) {
    for &b in buf {
        if b == b'\n' {
            putc(b'\r');
        }
        putc(b);
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

/// Print raw bytes (FDT strings), with no LF translation: these are property
/// values, not lines.
pub fn put_bytes(s: &[u8]) {
    for &b in s {
        putc(b);
    }
}

pub fn put_hex(mut v: u64) {
    const HEX: &[u8] = b"0123456789abcdef";
    puts("0x");
    let mut buf = [0u8; 16];
    let mut n = 0;
    if v == 0 {
        putc(b'0');
        return;
    }
    while v > 0 {
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
    let mut buf = [0u8; 20];
    let mut n = 0;
    if v == 0 {
        putc(b'0');
        return;
    }
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

/// Boot splash: ASCII logo plus the release signature line. One copy for both
/// ports — it is the same program either way.
pub const LOGO: &str = concat!(
    "  __                _\n",
    " / _| __ _ _ __  | |_ _   _  __ _ _ __\n",
    "| |_ / _` | '  \\| __| | | |/ _` | '  \\\n",
    "|  _| (_| | | | | |_| |_| | (_| | | | |\n",
    "|_|  \\__,_|_| |_|\\__|\\__,_|\\__,_|_| |_|\n",
    "  fantuan v0.0.4 - fantuan-is-mtf\n",
);
