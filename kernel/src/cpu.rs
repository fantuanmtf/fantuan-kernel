//! Minimal CPU-state helpers — arch code, not drivers (DESIGN.md §2.1).

use core::arch::asm;

#[inline]
pub fn sti() {
    unsafe { asm!("sti", options(nomem, preserves_flags)) }
}

/// Save RFLAGS and disable interrupts. Pair with irq_restore.
#[inline]
pub fn irq_save() -> u64 {
    let flags: u64;
    unsafe {
        asm!("pushfq; cli; pop {}", out(reg) flags, options(nomem));
    }
    flags
}

#[inline]
pub fn irq_restore(flags: u64) {
    unsafe {
        asm!("push {}; popfq", in(reg) flags, options(nomem));
    }
}
