//! Single-core interrupt-safe spinlock for mm state (M8.3a).
//!
//! The lock is taken with interrupts disabled on the local core: an IRQ
//! handler that needs the same lock (the scheduler path will free frames
//! from IRQ0) can therefore never preempt a task mid-update and deadlock.
//! Per-CPU locks become necessary only with SMP.

use core::sync::atomic::{AtomicBool, Ordering};

pub struct IrqLock {
    lock: &'static AtomicBool,
    flags: u64,
}

impl IrqLock {
    pub fn acquire(lock: &'static AtomicBool) -> Self {
        let flags = crate::cpu::irq_save();
        while lock.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        Self { lock, flags }
    }
}

impl Drop for IrqLock {
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
        crate::cpu::irq_restore(self.flags);
    }
}
