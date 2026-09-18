//! Memory hooks for the shared diagnostics: each kernel installs its
//! physical-to-virtual alias at boot. The default is the identity, which is
//! harmless for the low direct-map window the RAM test borrows frames from.

use core::sync::atomic::{AtomicUsize, Ordering};

static PHYS_TO_VIRT: AtomicUsize = AtomicUsize::new(0);

/// Install the phys->virt alias (called once per kernel boot).
pub fn set_phys_to_virt(f: fn(u64) -> u64) {
    PHYS_TO_VIRT.store(f as usize, Ordering::Release);
}

/// Translate a physical address for the running kernel (identity until
/// installed).
pub fn phys_to_virt(p: u64) -> u64 {
    let f = PHYS_TO_VIRT.load(Ordering::Acquire);
    if f == 0 {
        return p;
    }
    let f: fn(u64) -> u64 = unsafe { core::mem::transmute(f) };
    f(p)
}
