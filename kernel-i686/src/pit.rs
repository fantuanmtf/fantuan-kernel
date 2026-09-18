//! 8254 PIT channel 0 at 100 Hz, driving the scheduler tick on i686.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::cpu::outb;

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Program channel 0 in mode 3 with the given frequency.
pub fn init(hz: u32) {
    let divisor = (1_193_182u64 / hz.max(1) as u64) as u16;
    outb(0x43, 0x36);
    outb(0x40, divisor as u8);
    outb(0x40, (divisor >> 8) as u8);
}

pub fn tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}
