//! Input hook for the shared shell: each kernel installs its merged input
//! source (serial + PS/2, UART, ...) at boot. Until then the default reports
//! no input, so an early caller simply sees an empty channel.

use core::sync::atomic::{AtomicUsize, Ordering};

static POLL: AtomicUsize = AtomicUsize::new(0);

/// Install the non-blocking byte source (called once per kernel boot).
pub fn set_poll(f: fn() -> Option<u8>) {
    POLL.store(f as usize, Ordering::Release);
}

/// Non-blocking read of one byte from any input channel (none until installed).
pub fn poll_byte() -> Option<u8> {
    let p = POLL.load(Ordering::Acquire);
    if p == 0 {
        return None;
    }
    let f: fn() -> Option<u8> = unsafe { core::mem::transmute(p) };
    f()
}
