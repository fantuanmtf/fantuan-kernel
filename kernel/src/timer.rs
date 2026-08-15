//! Timer bookkeeping: IRQ0 ticks at 100 Hz (see pit::init_timer).
//!
//! Every 10 ticks (100 ms quantum) the timer drives the round-robin
//! scheduler (M3, DESIGN.md §4.6); every 10 s it prints a heartbeat with the
//! context-switch counter.

use core::fmt::Write;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::serial::{self, Serial};
use crate::task;

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Called from the IRQ0 handler (interrupts.rs).
pub fn tick() {
    let n = TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    if n % 1000 == 0 {
        let mut s = Serial::new(serial::COM1);
        let _ = writeln!(s, "tick: {} s (switches {})", n / 100, task::SWITCHES.load(Ordering::Relaxed));
    }
    if n % 10 == 0 {
        task::schedule();
    }
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}
