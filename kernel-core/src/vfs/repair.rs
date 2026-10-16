//! The write gate: repair mode, the `RepairToken` capability that proves it is
//! on, and the single 8.3-file write entry point that requires the token.
//!
//! This is the rescue iron rule as a type: no disk write outside a token, and
//! the only issuer is `repair_guard`, which refuses while repair mode is off.
//! The token's field stays private to this module, which is what makes "a
//! write without explicit operator consent does not compile" true rather than
//! aspirational. A child of `vfs`; the paths are re-exported at `vfs::`.

use core::sync::atomic::{AtomicBool, Ordering};

use super::fat;

/// Repair mode: writes are impossible until this is explicitly enabled.
/// The rescue iron rule — the kernel never writes what the user did not ask
/// it to fix.
static REPAIR_MODE: AtomicBool = AtomicBool::new(false);

pub fn enable_repair_mode() {
    REPAIR_MODE.store(true, Ordering::Relaxed);
}

/// Whether writes are currently permitted. The repair path checks this, so a
/// stray call can never write with repair mode off.
pub fn repair_mode() -> bool {
    REPAIR_MODE.load(Ordering::Relaxed)
}

/// Capability token proving repair mode is on. The FAT write API takes this
/// token, so a write without explicit operator consent does not compile; the
/// shell's `grub-fix repair` + YES is the only enabler.
#[derive(Clone, Copy)]
pub struct RepairToken(());

/// Obtain the write capability; None while repair mode is off.
pub fn repair_guard() -> Option<RepairToken> {
    if repair_mode() {
        Some(RepairToken(()))
    } else {
        None
    }
}

/// Create or overwrite an 8.3 file in the given directory — gated behind
/// repair mode.
pub fn write_file(fs: &fat::Fat32, dir_cluster: u32, name: &[u8; 11], data: &[u8]) -> bool {
    let Some(token) = repair_guard() else {
        return false;
    };
    fs.write_file(dir_cluster, name, data, &token)
}
