//! PS/2 mouse driver (M13-3, i686): bring up the 8042 aux port (the i686
//! kernel has no keyboard driver, so this also brings the controller up),
//! IRQ12, defaults and 3-byte packet mode; decode packets through the shared
//! `kernel-core::mouse` decoder and feed pointer events into the input ring.
//! The i686 runtime input path is untested (the smokes only build and boot it);
//! every wait is bounded so a missing mouse degrades gracefully.

use crate::cpu::{inb, outb};
use kernel_core::mouse::MouseDecoder;

const DATA: u16 = 0x60;
const CMD: u16 = 0x64;
const OUT_FULL: u8 = 0x01;
const IN_FULL: u8 = 0x02;
const ACK: u8 = 0xFA;

static mut DECODER: MouseDecoder = MouseDecoder::new();

fn status() -> u8 {
    inb(CMD)
}

fn wait_in_clear() -> bool {
    for _ in 0..1_000_000 {
        if status() & IN_FULL == 0 {
            return true;
        }
    }
    false
}

fn read_timeout() -> Option<u8> {
    for _ in 0..1_000_000 {
        if status() & OUT_FULL != 0 {
            return Some(inb(DATA));
        }
    }
    None
}

fn flush() {
    for _ in 0..16 {
        if status() & OUT_FULL == 0 {
            break;
        }
        inb(DATA);
    }
}

fn cmd(c: u8) -> bool {
    if !wait_in_clear() {
        return false;
    }
    outb(CMD, c);
    true
}

fn write_data(b: u8) -> bool {
    if !wait_in_clear() {
        return false;
    }
    outb(DATA, b);
    true
}

fn aux_write(b: u8) -> bool {
    cmd(0xD4) && write_data(b)
}

/// Bring the controller up for the mouse only: enable the aux port and IRQ12,
/// disable the keyboard clock (there is no keyboard handler) and reset the
/// mouse into 3-byte data-reporting mode. Returns false on any timeout.
pub fn init() -> bool {
    flush();
    // Controller self-test (0xAA -> 0x55), then read the config byte.
    if !cmd(0xAA) || read_timeout() != Some(0x55) {
        return false;
    }
    if !cmd(0x20) {
        return false;
    }
    let Some(cfg) = read_timeout() else {
        return false;
    };
    // Aux IRQ (bit 1) + aux clock (clear bit 5); disable the keyboard clock
    // (bit 4) and leave the keyboard IRQ (bit 0) off — no keyboard handler.
    let new = (cfg | 0x02 | 0x10) & !0x20 & !0x01;
    if !cmd(0x60) || !write_data(new) {
        return false;
    }
    if !cmd(0xA8) {
        return false;
    }
    if !aux_write(0xFF) {
        return false;
    }
    if read_timeout() != Some(ACK) {
        return false;
    }
    if read_timeout() != Some(0xAA) {
        return false;
    }
    if read_timeout() != Some(0x00) {
        return false;
    }
    if !aux_write(0xF6) || read_timeout() != Some(ACK) {
        return false;
    }
    if !aux_write(0xF4) || read_timeout() != Some(ACK) {
        return false;
    }
    crate::pic::unmask(12);
    true
}

/// IRQ12 handler: read one byte, feed the decoder, push a pointer event.
pub fn irq() {
    if status() & OUT_FULL == 0 {
        return;
    }
    let b = inb(DATA);
    if let Some(p) = unsafe { (*core::ptr::addr_of_mut!(DECODER)).push(b) } {
        kernel_core::input_ring::push_pointer(p.dx, p.dy, p.buttons);
    }
}
