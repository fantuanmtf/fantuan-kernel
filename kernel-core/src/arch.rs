//! Arch hooks for the shared core. Each kernel installs them at boot before
//! the first allocation; until then save/restore are no-ops (early boot is
//! single-context with interrupts disabled where it matters).

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static SAVE: AtomicUsize = AtomicUsize::new(0);
static RESTORE: AtomicUsize = AtomicUsize::new(0);

/// Install the arch interrupt-state helpers (called once per kernel boot).
pub fn set_irq_ops(save: fn() -> u64, restore: fn(u64)) {
    SAVE.store(save as usize, Ordering::Release);
    RESTORE.store(restore as usize, Ordering::Release);
}

/// Save and disable interrupts (arch hook; no-op until installed).
pub fn irq_save() -> u64 {
    let p = SAVE.load(Ordering::Acquire);
    if p == 0 {
        return 0;
    }
    let f: fn() -> u64 = unsafe { core::mem::transmute(p) };
    f()
}

/// Restore the interrupt state saved by irq_save.
pub fn irq_restore(flags: u64) {
    let p = RESTORE.load(Ordering::Acquire);
    if p == 0 {
        return;
    }
    let f: fn(u64) = unsafe { core::mem::transmute(p) };
    f(flags);
}

/// Single-core interrupt-safe spinlock: acquired with interrupts disabled so
/// an IRQ handler needing the same lock cannot preempt a holder (per-CPU
/// locks only become necessary with SMP).
pub struct IrqLock {
    lock: &'static AtomicBool,
    flags: u64,
}

impl IrqLock {
    pub fn acquire(lock: &'static AtomicBool) -> Self {
        let flags = irq_save();
        while lock.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        Self { lock, flags }
    }
}

impl Drop for IrqLock {
    fn drop(&mut self) {
        self.lock.store(false, Ordering::Release);
        irq_restore(self.flags);
    }
}
