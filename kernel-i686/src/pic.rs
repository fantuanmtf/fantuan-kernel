//! 8259 PIC remap for the i686 kernel: master vectors 0x20..0x27, slave
//! 0x28..0x2F, everything masked except IRQ0 (PIT) and IRQ1 (keyboard).

use crate::cpu::{inb, outb};

const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

fn io_wait() {
    outb(0x80, 0);
}

pub fn remap() {
    let mask1 = inb(PIC1_DATA);
    let mask2 = inb(PIC2_DATA);

    outb(PIC1_CMD, 0x11);
    io_wait();
    outb(PIC2_CMD, 0x11);
    io_wait();
    outb(PIC1_DATA, 0x20); // master offset 32
    io_wait();
    outb(PIC2_DATA, 0x28); // slave offset 40
    io_wait();
    outb(PIC1_DATA, 0x04); // slave on IRQ2
    io_wait();
    outb(PIC2_DATA, 0x02);
    io_wait();
    outb(PIC1_DATA, 0x01); // 8086/88 mode
    io_wait();
    outb(PIC2_DATA, 0x01);
    io_wait();

    // Keep the previous mask for the slave, unmask IRQ0 only on the master
    // for now (IRQ1 is unmasked by the keyboard init later).
    outb(PIC1_DATA, (mask1 & 0xF8) | 0x00);
    outb(PIC2_DATA, mask2);
}

/// Unmask one IRQ line (0..15). Used by the keyboard step (M10-4b3).
#[allow(dead_code)]
pub fn unmask(irq: u8) {
    let (port, bit) = if irq < 8 { (PIC1_DATA, irq) } else { (PIC2_DATA, irq - 8) };
    let mask = inb(port) & !(1u8 << bit);
    outb(port, mask);
}

/// End-of-interrupt for a remapped IRQ number (0..15).
pub fn eoi(irq: u8) {
    if irq >= 8 {
        outb(PIC2_CMD, 0x20);
    }
    outb(PIC1_CMD, 0x20);
}
