//! Storage-driver hooks for the shared diagnostics and boot repair. Each
//! kernel installs its accessors at boot; the defaults describe "no drive",
//! so a kernel without a storage stack (or before bring-up) reports
//! harmlessly: a null handle, an empty name, no identity and no BDF.

use core::ffi::c_void;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Generic identity as the C drivers report it (driver.h struct blk_identity).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BlkIdentity {
    pub model: [u8; 41],
    pub serial: [u8; 21],
    pub sectors: u64,
    pub ssd: i32,
}

impl BlkIdentity {
    pub const EMPTY: BlkIdentity = BlkIdentity {
        model: [0; 41],
        serial: [0; 21],
        sectors: 0,
        ssd: 0,
    };
}

/// The arch storage accessors the shared code needs (installed once at boot).
pub struct DrvOps {
    pub drive_handle: fn() -> *mut c_void,
    pub drive_name: fn() -> &'static str,
    pub drive_identity: fn() -> Option<BlkIdentity>,
    /// Bus/device of the controller behind the active handle; u32::MAX when
    /// unknown, which never matches a device-path PCI node.
    pub storage_bdf: fn() -> u32,
}

static HANDLE: AtomicUsize = AtomicUsize::new(0);
static NAME: AtomicUsize = AtomicUsize::new(0);
static IDENTITY: AtomicUsize = AtomicUsize::new(0);
static BDF: AtomicUsize = AtomicUsize::new(0);

/// Install the storage accessors (called once per kernel boot).
pub fn set_ops(ops: DrvOps) {
    HANDLE.store(ops.drive_handle as usize, Ordering::Release);
    NAME.store(ops.drive_name as usize, Ordering::Release);
    IDENTITY.store(ops.drive_identity as usize, Ordering::Release);
    BDF.store(ops.storage_bdf as usize, Ordering::Release);
}

/// Handle of the active drive (null until installed).
pub fn drive_handle() -> *mut c_void {
    let p = HANDLE.load(Ordering::Acquire);
    if p == 0 {
        return core::ptr::null_mut();
    }
    let f: fn() -> *mut c_void = unsafe { core::mem::transmute(p) };
    f()
}

/// Name of the driver behind the active handle ("" until installed).
pub fn drive_name() -> &'static str {
    let p = NAME.load(Ordering::Acquire);
    if p == 0 {
        return "";
    }
    let f: fn() -> &'static str = unsafe { core::mem::transmute(p) };
    f()
}

/// Driver-decoded identity of the active drive (None until installed).
pub fn drive_identity() -> Option<BlkIdentity> {
    let p = IDENTITY.load(Ordering::Acquire);
    if p == 0 {
        return None;
    }
    let f: fn() -> Option<BlkIdentity> = unsafe { core::mem::transmute(p) };
    f()
}

/// Bus/device of the storage controller (u32::MAX until installed).
pub fn storage_bdf() -> u32 {
    let p = BDF.load(Ordering::Acquire);
    if p == 0 {
        return u32::MAX;
    }
    let f: fn() -> u32 = unsafe { core::mem::transmute(p) };
    f()
}
