//! Arch hooks for the shared core. Each kernel installs them at boot before
//! the first allocation; until then save/restore are no-ops (early boot is
//! single-context with interrupts disabled where it matters).

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static SAVE: AtomicUsize = AtomicUsize::new(0);
static RESTORE: AtomicUsize = AtomicUsize::new(0);
static ENABLE: AtomicUsize = AtomicUsize::new(0);
static IDLE: AtomicUsize = AtomicUsize::new(0);

/// Install the arch interrupt-state helpers (called once per kernel boot).
pub fn set_irq_ops(save: fn() -> u64, restore: fn(u64)) {
    SAVE.store(save as usize, Ordering::Release);
    RESTORE.store(restore as usize, Ordering::Release);
}

/// Install the unconditional interrupt-enable helper (called at boot with
/// set_irq_ops). Exited tasks use it to let timer ticks wake the sleepers
/// that will reap them; without it, `exit_with` would spin with IF=0 and no
/// tick could ever advance.
pub fn set_irq_enable(f: fn()) {
    ENABLE.store(f as usize, Ordering::Release);
}

/// Unconditionally enable interrupts (arch hook; no-op until installed).
pub fn irq_enable() {
    let p = ENABLE.load(Ordering::Acquire);
    if p == 0 {
        return;
    }
    let f: fn() = unsafe { core::mem::transmute(p) };
    f()
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

/// Install the idle instruction (hlt on x86_64, wfi on riscv64) used by the
/// shared shell loop (called once per kernel boot).
pub fn set_idle(f: fn()) {
    IDLE.store(f as usize, Ordering::Release);
}

/// Idle until the next interrupt (spin until installed, which is safe: the
/// hook is installed before the first interactive caller).
pub fn idle() {
    let p = IDLE.load(Ordering::Acquire);
    if p == 0 {
        core::hint::spin_loop();
        return;
    }
    let f: fn() = unsafe { core::mem::transmute(p) };
    f()
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
