//! 8259 PIC: remap IRQs 0-15 to vectors 0x20-0x2F, mask, EOI.

use crate::consts::{
    IRQ_TIMER, PIC1_CMD, PIC1_DATA, PIC1_OFFSET, PIC2_CMD, PIC2_DATA, PIC2_OFFSET,
};
use crate::port::{inb, outb};

pub fn init() {
    unsafe {
        // ICW1: init + ICW4
        outb(PIC1_CMD, 0x11);
        outb(PIC2_CMD, 0x11);
        // ICW2: vector offsets
        outb(PIC1_DATA, PIC1_OFFSET);
        outb(PIC2_DATA, PIC2_OFFSET);
        // ICW3: cascade wiring (slave on master IRQ2)
        outb(PIC1_DATA, 0x04);
        outb(PIC2_DATA, 0x02);
        // ICW4: 8086 mode
        outb(PIC1_DATA, 0x01);
        outb(PIC2_DATA, 0x01);
        // Mask everything except the timer on the master.
        outb(PIC1_DATA, !(1u8 << (IRQ_TIMER - PIC1_OFFSET)));
        outb(PIC2_DATA, 0xFF);
    }
}

/// Unmask one IRQ line (IRQ = PIC vector offset, e.g. IRQ_TIMER).
pub fn unmask(irq: u8) {
    unsafe {
        if irq < PIC2_OFFSET {
            outb(PIC1_DATA, inb(PIC1_DATA) & !(1u8 << (irq - PIC1_OFFSET)));
        } else {
            outb(PIC2_DATA, inb(PIC2_DATA) & !(1u8 << (irq - PIC2_OFFSET)));
            outb(PIC1_DATA, inb(PIC1_DATA) & !(1u8 << 2)); // master cascade
        }
    }
}

pub fn eoi(vector: u8) {
    unsafe {
        if vector >= PIC2_OFFSET {
            outb(PIC2_CMD, 0x20);
        }
        outb(PIC1_CMD, 0x20);
    }
}
