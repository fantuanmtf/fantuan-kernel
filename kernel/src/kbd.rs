//! PS/2 keyboard bridge (M8.5a): thin Rust wrappers around the C i8042
//! driver. IRQ1 dispatch calls `irq()`; the merged input source polls
//! `getc()`.

extern "C" {
    fn kbd_init() -> i32;
    fn kbd_irq();
    fn kbd_getc() -> i32;
}

/// Configure the i8042 controller; false when no keyboard is present.
pub fn init() -> bool {
    unsafe { kbd_init() == 0 }
}

/// IRQ1 handler hook (called from isr_dispatch).
pub fn irq() {
    unsafe { kbd_irq() }
}

/// One translated ASCII byte, or None when the buffer is empty.
pub fn getc() -> Option<u8> {
    match unsafe { kbd_getc() } {
        -1 => None,
        c => Some(c as u8),
    }
}
