//! Kernel log sink (M9.4): shared modules format through `Log`, and each
//! kernel installs the byte sink at boot (x86 serial/GOP mirror, riscv UART).
//! The sink owns newline translation so shared code only ever emits LF.

use core::fmt;

static mut SINK: fn(&[u8]) = |_| {};

pub fn set_sink(f: fn(&[u8])) {
    unsafe { SINK = f };
}

pub fn put(bytes: &[u8]) {
    unsafe { (SINK)(bytes) };
}

pub fn line(s: &str) {
    put(s.as_bytes());
    put(b"\n");
}

/// `core::fmt::Write` adapter for the shared diagnostics.
pub struct Log;

impl Log {
    pub const fn new() -> Log {
        Log
    }
    pub fn write(&self, bytes: &[u8]) -> usize {
        put(bytes);
        bytes.len()
    }
}

impl fmt::Write for Log {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        put(s.as_bytes());
        Ok(())
    }
}
