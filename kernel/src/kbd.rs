//! PS/2 keyboard bridge (M8.5a): thin Rust wrappers around the C i8042
//! driver. IRQ1 dispatch calls `irq()`; the merged input source polls
//! `getc()`. M13-3: `irq_event()` also decodes the packed key event the C
//! driver returns, which the dispatch feeds into the shared input ring.

extern "C" {
    fn kbd_init() -> i32;
    fn kbd_irq() -> i32;
    fn kbd_getc() -> i32;
}

/// One decoded key event (the shared input-ring view of a scancode). Only the
/// graphics build consumes key events, so the struct is gated with it.
#[cfg(kconfig_graphics)]
pub struct KeyEvent {
    pub scancode: u8,
    pub set: u8,
    pub pressed: bool,
    pub ascii: u8,
}

/// Configure the i8042 controller; false when no keyboard is present.
pub fn init() -> bool {
    unsafe { kbd_init() == 0 }
}

/// IRQ1 handler hook (non-graphics): queue the translated ASCII byte.
#[cfg(not(kconfig_graphics))]
pub fn irq() {
    unsafe {
        kbd_irq();
    }
}

/// IRQ1 handler hook that also returns the decoded key event for the input
/// ring (still queues the ASCII byte, so the serial shell path is unchanged).
#[cfg(kconfig_graphics)]
pub fn irq_event() -> Option<KeyEvent> {
    let v = unsafe { kbd_irq() };
    if v < 0 {
        return None;
    }
    Some(KeyEvent {
        scancode: (v & 0x7F) as u8,
        pressed: (v >> 8) & 1 == 1,
        set: ((v >> 16) & 0xFF) as u8,
        ascii: ((v >> 24) & 0xFF) as u8,
    })
}

/// One translated ASCII byte, or None when the buffer is empty.
pub fn getc() -> Option<u8> {
    match unsafe { kbd_getc() } {
        -1 => None,
        c => Some(c as u8),
    }
}
