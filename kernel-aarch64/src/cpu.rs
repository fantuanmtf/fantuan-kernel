//! aarch64 CPU-state helpers: DAIF save/restore for the shared allocator
//! lock, WFI, and the early EL1 setup (FP/SIMD access, SP_EL1).

use core::arch::asm;

/// Mask IRQs (PSTATE.I) and return the previous DAIF value.
pub fn irq_save() -> u64 {
    let flags: u64;
    unsafe {
        asm!("mrs {}, daif", out(reg) flags, options(nomem, nostack));
        asm!("msr daifset, #2", options(nomem, nostack));
    }
    flags
}

/// Restore the DAIF value saved by irq_save (bit 7 = I).
pub fn irq_restore(flags: u64) {
    unsafe { asm!("msr daif, {}", in(reg) flags, options(nomem, nostack)) };
}

/// Idle until the next interrupt; installed as the shared idle hook.
pub fn idle() {
    unsafe { asm!("wfi", options(nomem, nostack)) };
}

/// Early EL1 setup: allow FP/SIMD (the compiler may use NEON in optimized
/// integer code) and make sure SP_EL1 is selected.
pub fn init() {
    unsafe {
        asm!("msr cpacr_el1, {}", in(reg) 3u64 << 20, options(nostack));
        asm!("msr spsel, #1", options(nostack));
        asm!("isb", options(nostack));
    }
}
