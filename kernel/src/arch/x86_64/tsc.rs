//! TSC-based busy sleep, calibrated once against the PIT.
//!
//! Calibration MUST run before pit::init_timer() (which repurposes channel 0).
//! The PIT channel-0 OUT-pin poll needs no CPU-speed assumptions, so the
//! measurement is exact regardless of the CPU model.

use core::arch::x86_64::_rdtsc;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::consts::PIT_FREQ_HZ;
use crate::port::{inb, outb};

static TSC_HZ: AtomicU64 = AtomicU64::new(0);

/// Measure the TSC frequency with a PIT channel-0 one-shot (≈55 ms).
pub fn calibrate() {
    const COUNT: u64 = 0xFFFF;
    let start = unsafe { _rdtsc() };
    unsafe {
        outb(0x43, 0x30); // channel 0, lobyte/hibyte, mode 0 (one-shot)
        outb(0x40, (COUNT & 0xFF) as u8);
        outb(0x40, (COUNT >> 8) as u8);
        loop {
            outb(0x43, 0xE2); // read-back: channel 0, status only
            if inb(0x40) & 0x80 != 0 {
                break; // OUT pin high = terminal count reached
            }
        }
    }
    let end = unsafe { _rdtsc() };
    TSC_HZ.store((end - start) * PIT_FREQ_HZ / COUNT, Ordering::Relaxed);
}

pub fn hz() -> u64 {
    TSC_HZ.load(Ordering::Relaxed)
}

/// Raw TSC read / tick conversion — used by the disk surface scan (§7).
#[allow(dead_code)]
pub fn now() -> u64 {
    unsafe { _rdtsc() }
}

#[allow(dead_code)]
pub fn to_nanos(ticks: u64) -> u128 {
    let hz = TSC_HZ.load(Ordering::Relaxed);
    if hz == 0 {
        return 0;
    }
    (ticks as u128) * 1_000_000_000u128 / (hz as u128)
}

/// Monotonic nanoseconds since boot: the x86 source for the shared clock
/// hook (0 until calibration).
pub fn now_ns() -> u64 {
    to_nanos(now()) as u64
}

/// Busy-wait sleep. Safe before calibration (TSC_HZ = 0 => returns at once).
pub fn sleep_ms(ms: u64) {
    let hz = TSC_HZ.load(Ordering::Relaxed);
    let deadline = unsafe { _rdtsc() }.wrapping_add(ms * hz / 1000);
    while unsafe { _rdtsc() } < deadline {
        spin_loop();
    }
}
