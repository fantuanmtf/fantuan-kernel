//! SBI timer at 100 Hz (M9.2b): every tick re-arms `set_timer`; every 1000
//! ticks a heartbeat is logged. The scheduler hook (M9.2c) slots in at the
//! same place the x86 IRQ0 handler calls its tick.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::sbi;
use crate::{put_dec, puts};

static TICKS: AtomicU64 = AtomicU64::new(0);
static mut TIMEBASE: u64 = 10_000_000;
static mut NEXT: u64 = 0;

/// Current `time` CSR value.
pub fn time() -> u64 {
    let t: u64;
    unsafe { asm!("rdtime {}", out(reg) t, options(nomem, nostack)) };
    t
}

/// Start the S-mode timer; TIMEBASE_HZ comes from the FDT (10 MHz on QEMU
/// virt when absent).
pub fn init(timebase_hz: u64) {
    unsafe {
        TIMEBASE = if timebase_hz == 0 { 10_000_000 } else { timebase_hz };
    }
    arm(time() + interval());
    unsafe {
        // STIE (sie bit 5) + SIE (sstatus bit 1).
        asm!("csrs sie, {}", in(reg) 1u64 << 5, options(nostack));
        asm!("csrs sstatus, {}", in(reg) 1u64 << 1, options(nostack));
    }
}

fn interval() -> u64 {
    unsafe { TIMEBASE / 100 }
}

fn arm(next: u64) {
    unsafe { NEXT = next };
    if let Err(err) = sbi::set_timer(next) {
        puts("timer: SBI set_timer failed err=");
        put_dec(err as u64);
        puts("\n");
    }
}

/// Heartbeats are a boot-verification aid: after the first 30 s they would
/// only interrupt the interactive shell prompt, so they stop.
const HEARTBEAT_TICKS: u64 = 3_000;

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Interrupt handler: count, re-arm, heartbeat every 10 s (first 30 s only).
pub fn tick() {
    let t = TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    arm(unsafe { NEXT } + interval());
    if t % 1000 == 0 && t <= HEARTBEAT_TICKS {
        puts("tick: ");
        put_dec(t / 100);
        puts(" s (SBI timer, interrupts on)\n");
    }
    // The scheduler runs from the trap context, exactly like x86 IRQ0.
    kernel_core::task::schedule();
}
