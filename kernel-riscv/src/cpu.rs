//! RISC-V CPU-state helpers: interrupt-state save/restore for the shared
//! allocator lock (sstatus.SIE) and WFI.

use core::arch::asm;

/// sstatus.SIE is bit 1.
pub fn irq_save() -> u64 {
    let s: u64;
    unsafe {
        asm!("csrr {}, sstatus", out(reg) s, options(nomem, nostack));
        asm!("csrci sstatus, 2", options(nomem, nostack));
    }
    s
}

pub fn irq_restore(flags: u64) {
    // Only re-enable interrupts when they were enabled before.
    if flags & 2 != 0 {
        unsafe { asm!("csrsi sstatus, 2", options(nomem, nostack)) };
    }
}

/// Unconditional sstatus.SIE set (exited-task idle loop).
pub fn irq_enable() {
    unsafe { asm!("csrsi sstatus, 2", options(nomem, nostack)) };
}

/// Idle until the next interrupt (SBI timer); installed as the shared idle
/// hook.
pub fn idle() {
    unsafe { asm!("wfi", options(nomem, nostack)) };
}
