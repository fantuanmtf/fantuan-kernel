//! PS/2 mouse driver (M13-3): bring up the 8042 aux port, IRQ12, defaults and
//! 3-byte packet mode, decode standard packets through the shared
//! `kernel-core::mouse` decoder and feed pointer events into the shared input
//! ring. Called after `kbd::init` (which configured the keyboard bits of the
//! controller config byte) and before interrupts are enabled; every wait is
//! bounded so an absent mouse degrades gracefully.

use crate::consts::IRQ_MOUSE;
use crate::port::{inb, outb};
use kernel_core::mouse::MouseDecoder;

const DATA: u16 = 0x60;
const CMD: u16 = 0x64;
const OUT_FULL: u8 = 0x01;
const IN_FULL: u8 = 0x02;
const ACK: u8 = 0xFA;

static mut DECODER: MouseDecoder = MouseDecoder::new();

fn status() -> u8 {
    unsafe { inb(CMD) }
}

fn wait_in_clear() -> bool {
    for _ in 0..1000 {
        if status() & IN_FULL == 0 {
            return true;
        }
        crate::tsc::sleep_ms(1);
    }
    false
}

fn read_timeout() -> Option<u8> {
    for _ in 0..1000 {
        if status() & OUT_FULL != 0 {
            return Some(unsafe { inb(DATA) });
        }
        crate::tsc::sleep_ms(1);
    }
    None
}

fn flush() {
    for _ in 0..16 {
        if status() & OUT_FULL == 0 {
            break;
        }
        unsafe {
            inb(DATA);
        }
    }
}

fn cmd(c: u8) -> bool {
    if !wait_in_clear() {
        return false;
    }
    unsafe { outb(CMD, c) };
    true
}

fn write_data(b: u8) -> bool {
    if !wait_in_clear() {
        return false;
    }
    unsafe { outb(DATA, b) };
    true
}

fn aux_write(b: u8) -> bool {
    cmd(0xD4) && write_data(b)
}

/// Enable the aux port, reset the mouse and put it in 3-byte data-reporting
/// mode. Returns false when any step times out; the keyboard/serial paths are
/// unaffected either way.
pub fn init() -> bool {
    flush();
    // Read the controller config byte; the keyboard bits were set by kbd_init.
    if !cmd(0x20) {
        return false;
    }
    let Some(cfg) = read_timeout() else {
        return false;
    };
    // Enable aux IRQ (bit 1) and the aux clock (clear bit 5).
    let new = (cfg | 0x02) & !0x20;
    if !cmd(0x60) || !write_data(new) {
        return false;
    }
    // Enable the aux port, then reset the mouse: ACK, 0xAA self-test, 0x00 ID.
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
    // Set defaults, then enable data reporting (standard 3-byte packets).
    if !aux_write(0xF6) || read_timeout() != Some(ACK) {
        return false;
    }
    if !aux_write(0xF4) || read_timeout() != Some(ACK) {
        return false;
    }
    crate::pic::unmask(IRQ_MOUSE);
    true
}

/// IRQ12 handler: read one byte, feed the decoder, push a pointer event.
pub fn irq() {
    if status() & OUT_FULL == 0 {
        return;
    }
    let b = unsafe { inb(DATA) };
    if let Some(p) = unsafe { (*core::ptr::addr_of_mut!(DECODER)).push(b) } {
        kernel_core::input_ring::push_pointer(p.dx, p.dy, p.buttons);
    }
}
