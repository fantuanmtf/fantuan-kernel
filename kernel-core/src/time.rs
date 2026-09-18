//! Monotonic-time hook for the shared diagnostics: kernels with a clock
//! install their nanosecond source at boot. The default 0 lets timer-relative
//! checks degrade to "not slow" on kernels without one.

use core::sync::atomic::{AtomicUsize, Ordering};

static NOW_NS: AtomicUsize = AtomicUsize::new(0);

/// Install the monotonic-nanoseconds source (called once per kernel boot).
pub fn set_ns(f: fn() -> u64) {
    NOW_NS.store(f as usize, Ordering::Release);
}

/// Monotonic nanoseconds since an unspecified epoch (0 until installed).
pub fn now_ns() -> u64 {
    let p = NOW_NS.load(Ordering::Acquire);
    if p == 0 {
        return 0;
    }
    let f: fn() -> u64 = unsafe { core::mem::transmute(p) };
    f()
}
