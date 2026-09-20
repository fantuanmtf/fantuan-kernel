//! ARM generic timer at 100 Hz (M11 R9a): the EL1 physical timer
//! (CNTP_TVAL_EL0) re-armed every tick, PPI 30 through GICv2. CNTFRQ_EL0 is
//! read from the CPU (62.5 MHz on QEMU virt), not assumed. The scheduler
//! runs from the IRQ context exactly like x86 IRQ0 / riscv's SBI tick.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::{put_dec, puts};

static TICKS: AtomicU64 = AtomicU64::new(0);
static mut INTERVAL: u64 = 625_000;
static mut FREQ: u64 = 62_500_000;

/// Counter frequency; 62.5 MHz is the QEMU virt fallback if CNTFRQ reads 0.
pub fn freq() -> u64 {
    let v: u64;
    unsafe { asm!("mrs {}, cntfrq_el0", out(reg) v, options(nomem, nostack)) };
    if v == 0 {
        62_500_000
    } else {
        v
    }
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

fn arm(interval: u64) {
    unsafe {
        asm!("msr cntp_tval_el0, {}", in(reg) interval, options(nostack));
        asm!("msr cntp_ctl_el0, {}", in(reg) 1u64, options(nostack)); // ENABLE, IMASK=0
        asm!("isb", options(nostack));
    }
}

/// Start the tick, enable PPI 30 and unmask IRQs; returns CNTFRQ.
pub fn init() -> u64 {
    let f = freq();
    unsafe {
        FREQ = f;
        INTERVAL = (f / 100).max(1);
    }
    arm(unsafe { INTERVAL });
    crate::gic::enable_irq(crate::gic::TIMER_IRQ);
    unsafe { asm!("msr daifclr, #2", options(nostack)) }; // PSTATE.I = 0
    f
}

/// Heartbeats are a boot-verification aid: after 30 s they would only
/// interrupt the interactive shell prompt, so they stop (shared C4 rule).
const HEARTBEAT_TICKS: u64 = 3_000;

/// IRQ handler: count, re-arm, heartbeat every 10 s (first 30 s only), then
/// hand over to the shared scheduler.
pub fn tick() {
    let t = TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    arm(unsafe { INTERVAL });
    if t % 1000 == 0 && t <= HEARTBEAT_TICKS && !kernel_core::heartbeat::quiet() {
        puts("tick: ");
        put_dec(t / 100);
        puts(" s (generic timer, ");
        put_dec(unsafe { FREQ / 1_000_000 });
        puts(" MHz, interrupts on)\n");
    }
    // M11 R9b: drive the NetBSD callout wheel when CONFIG_NET is on.
    #[cfg(kconfig_net)]
    crate::net::tick();
    kernel_core::task::schedule();
}
