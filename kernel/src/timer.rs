//! Timer bookkeeping: IRQ0 ticks at 100 Hz (see pit::init_timer).

use core::fmt::Write;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::serial::{self, Serial};

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Called from the IRQ0 handler (interrupts.rs).
pub fn tick() {
    let n = TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    if n % 100 == 0 {
        let mut s = Serial::new(serial::COM1);
        let _ = writeln!(s, "tick: {} s", n / 100);
    }
}
