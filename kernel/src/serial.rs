//! 16550 UART serial output on COM1 — ASCII, 8N1, 115200 (DESIGN.md §3).

use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::port::{inb, outb};

pub const COM1: u16 = 0x3F8;

/// Task-to-task mutual exclusion for multi-byte writes. Interrupt-context
/// writers (heartbeat, exceptions) deliberately bypass it: they must never
/// spin on a lock held by a preempted task on this CPU (classic single-core
/// deadlock). They only run at boot or on faults, so the rare interleave is
/// acceptable until M5's console subsystem.
static LOCK: AtomicBool = AtomicBool::new(false);

/// Write a slice atomically with respect to other tasks.
pub fn write_locked(buf: &[u8]) -> usize {
    while LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    let n = Serial::new(COM1).write(buf);
    LOCK.store(false, Ordering::Release);
    n
}

/// Minimal no-formatting debug helpers (used on paths where fmt is unwelcome).
pub fn line(s: &str) {
    let ser = Serial::new(COM1);
    let _ = ser.write(s.as_bytes());
    let _ = ser.write(b"\r\n");
}

pub fn hex(v: u64) {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let ser = Serial::new(COM1);
    let mut buf = [0u8; 18];
    buf[0] = b'0';
    buf[1] = b'x';
    let mut n = 2;
    let mut started = false;
    for shift in (0..16).rev() {
        let nib = ((v >> (shift * 4)) & 0xF) as usize;
        if nib != 0 || started || shift == 0 {
            buf[n] = HEX[nib];
            n += 1;
            started = true;
        }
    }
    let _ = ser.write(&buf[..n]);
    let _ = ser.write(b"\r\n");
}

pub struct Serial {
    port: u16,
}

impl Serial {
    pub const fn new(port: u16) -> Self {
        Self { port }
    }

    pub fn init(&mut self) {
        unsafe {
            outb(self.port + 1, 0x00); // disable interrupts
            outb(self.port + 3, 0x80); // DLAB
            outb(self.port + 0, 0x01); // divisor low  -> 115200 baud
            outb(self.port + 1, 0x00); // divisor high
            outb(self.port + 3, 0x03); // 8N1
            outb(self.port + 2, 0xC7); // FIFO: enable, clear, 14-byte threshold
            outb(self.port + 4, 0x0B); // DTR | RTS | OUT2
        }
    }

    fn putc(&self, c: u8) {
        unsafe {
            // Wait until the transmit holding register is empty (LSR bit 5).
            while inb(self.port + 5) & 0x20 == 0 {}
            outb(self.port, c);
        }
    }

    /// Write a byte slice to the port (debug channel; no line discipline).
    pub fn write(&self, buf: &[u8]) -> usize {
        for &b in buf {
            self.putc(b);
        }
        buf.len()
    }
}

impl fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for &b in s.as_bytes() {
            if b == b'\n' {
                self.putc(b'\r');
            }
            self.putc(b);
        }
        Ok(())
    }
}
