//! 16550 UART serial output on COM1 — ASCII, 8N1, 115200 (DESIGN.md §3).

use crate::port::{inb, outb};
use core::fmt;

pub const COM1: u16 = 0x3F8;

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
