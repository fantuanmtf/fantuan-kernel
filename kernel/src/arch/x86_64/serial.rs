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

/// Mirror serial output onto the GOP console (M8.5b). Enabled once the shell
/// starts, so boot logs (already written to the console directly) are not
/// duplicated. IRQ-context serial writes mirror too; the console write is
/// plain memory traffic.
static MIRROR: AtomicBool = AtomicBool::new(false);

/// Freeze flag (M13-2): the graphics demo freezes the mirror while it animates
/// so its box is the only thing moving on screen during the smoke's
/// screendumps; serial output keeps flowing, it just stops drawing.
static MIRROR_FROZEN: AtomicBool = AtomicBool::new(false);

pub fn enable_mirror() {
    MIRROR.store(true, Ordering::Release);
}

pub fn freeze_mirror() {
    MIRROR_FROZEN.store(true, Ordering::Release);
}

pub fn unfreeze_mirror() {
    MIRROR_FROZEN.store(false, Ordering::Release);
}

/// Write a slice atomically with respect to other tasks.
pub fn write_locked(buf: &[u8]) -> usize {
    while LOCK.swap(true, Ordering::Acquire) {
        core::hint::spin_loop();
    }
    let n = Serial::new(COM1).write(buf);
    LOCK.store(false, Ordering::Release);
    n
}

/// Byte sink for the shared logger (expands LF to CRLF, like line()).
/// Deliberately bypasses LOCK: this sink can run from a preemptible task
/// while a sys_write (interrupt gate, IF clear) spins on LOCK; holding LOCK
/// across a task preemption would deadlock that spin. The old
/// Serial-formatted writers bypassed it the same way.
pub fn log_bytes(buf: &[u8]) {
    let ser = Serial::new(COM1);
    let mut start = 0;
    for (i, &b) in buf.iter().enumerate() {
        if b == b'\n' {
            if start < i {
                let _ = ser.write(&buf[start..i]);
            }
            let _ = ser.write(b"\r\n");
            start = i + 1;
        }
    }
    if start < buf.len() {
        let _ = ser.write(&buf[start..]);
    }
}

/// Byte sink for the shared raw logger: LF->CRLF with no GOP mirror (the
/// graphics demo's damage accounting must not touch the screen).
pub fn log_bytes_raw(buf: &[u8]) {
    let ser = Serial::new(COM1);
    let mut start = 0;
    for (i, &b) in buf.iter().enumerate() {
        if b == b'\n' {
            if start < i {
                ser.write_raw(&buf[start..i]);
            }
            ser.write_raw(b"\r\n");
            start = i + 1;
        }
    }
    if start < buf.len() {
        ser.write_raw(&buf[start..]);
    }
}

pub fn line(s: &str) {
    let ser = Serial::new(COM1);
    let _ = ser.write(s.as_bytes());
    let _ = ser.write(b"\r\n");
}pub fn hex(v: u64) {
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
        if MIRROR.load(Ordering::Relaxed) && !MIRROR_FROZEN.load(Ordering::Relaxed) {
            #[cfg(kconfig_graphics)]
            crate::console::global_putc(c);
        }
    }

    /// Byte to the port with no GOP mirror (the graphics demo's damage
    /// accounting must not touch the screen).
    fn putc_raw(&self, c: u8) {
        unsafe {
            while inb(self.port + 5) & 0x20 == 0 {}
            outb(self.port, c);
        }
    }

    /// Write a byte slice to the port without mirroring.
    fn write_raw(&self, buf: &[u8]) -> usize {
        for &b in buf {
            self.putc_raw(b);
        }
        buf.len()
    }

    /// Non-blocking read of one received byte (LSR bit 0 = data ready).
    pub fn read(&self) -> Option<u8> {
        unsafe {
            if inb(self.port + 5) & 0x01 != 0 {
                Some(inb(self.port))
            } else {
                None
            }
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
