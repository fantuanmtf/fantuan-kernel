//! Disk imager core (M12-2): raw block-device copy with mandatory SHA-256
//! verification, split from the `clone` shell command so the mechanism is
//! shared and the policy (plan, YES gate, repair token) stays in the shell.
//!
//! Flow: `plan()` prints the source/destination sizes and enforces the hard
//! size gate (source > destination is refused); the caller obtains a
//! `RepairToken` (the YES gate) and calls `run()`, which hashes the source
//! before the copy, streams the sectors through a fixed 1 MiB bounce buffer,
//! then re-reads the destination and compares the hashes. Reads and writes
//! are retried a bounded number of times; errors abort loudly. The
//! destination is never reported usable unless the hashes match.
//!
//! Device paths are the block registry's `blk0..blk3` (the same handles the
//! `DrvOps` storage layer uses); the C AHCI/virtio layers register one entry
//! per drive, so `clone blk0 blk1` is disk-to-disk on one controller.

use core::ffi::c_void;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::drv::BlkIdentity;
use crate::log::Log;

mod copy;
pub use copy::run;

/// Logical sector size; every supported transport reports 512-byte sectors.
pub const SECTOR: u64 = 512;
/// Registry size (drivers/c/blk.c MAX_BLK_DEVS); device paths are blk0..blk3.
pub const MAX_DEVS: usize = 4;
/// Fixed copy/hash buffer: the M12 design's 1 MiB boundary (the C drivers
/// split it into per-sector DMA transfers internally).
const BUF_BYTES: usize = 1 << 20;
/// Bounded retries for one read/write before the copy aborts.
const RETRIES: u32 = 3;

static CANCEL: AtomicBool = AtomicBool::new(false);

extern "C" {
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
    fn blk_write(dev: *mut c_void, lba: u64, buf: *const c_void, sectors: usize) -> i32;
    fn blk_open(index: usize) -> *mut c_void;
    fn blk_name(dev: *mut c_void) -> *const u8;
    fn blk_identity(dev: *mut c_void, out: *mut BlkIdentity) -> i32;
}

/// One opened raw block device.
pub struct Dev {
    pub handle: *mut c_void,
    pub index: usize,
    pub name: &'static str,
    pub sectors: u64,
}

/// Parse a device path (`blk0`..`blk3`) into a registry index.
pub fn parse_path(path: &[u8]) -> Option<usize> {
    let rest = path.strip_prefix(b"blk")?;
    if rest.is_empty() {
        return None;
    }
    let mut n = 0usize;
    for &c in rest {
        if !c.is_ascii_digit() {
            return None;
        }
        n = n * 10 + (c - b'0') as usize;
        if n >= MAX_DEVS {
            return None;
        }
    }
    Some(n)
}

fn dev_name(handle: *mut c_void) -> &'static str {
    let p = unsafe { blk_name(handle) };
    if p.is_null() {
        return "?";
    }
    let mut n = 0;
    while n < 15 && unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    core::str::from_utf8(unsafe { core::slice::from_raw_parts(p, n) }).unwrap_or("?")
}

/// Open registry entry INDEX; None when absent or identity-unreadable.
pub fn open(index: usize) -> Option<Dev> {
    let handle = unsafe { blk_open(index) };
    if handle.is_null() {
        return None;
    }
    let mut id = BlkIdentity::EMPTY;
    if unsafe { blk_identity(handle, &mut id) } != 0 || id.sectors == 0 {
        return None;
    }
    Some(Dev { handle, index, name: dev_name(handle), sectors: id.sectors })
}

/// Print the plan (source/destination/size/sector count, hashes performed)
/// and apply the hard size gate. False when the copy must not start.
pub fn plan(s: &mut Log, src: &Dev, dst: &Dev) -> bool {
    let _ = writeln!(
        s,
        "clone: plan src=blk{} ({}) {} sectors, {} KiB",
        src.index,
        src.name,
        src.sectors,
        src.sectors * SECTOR / 1024
    );
    let _ = writeln!(
        s,
        "clone: plan dst=blk{} ({}) {} sectors, {} KiB",
        dst.index,
        dst.name,
        dst.sectors,
        dst.sectors * SECTOR / 1024
    );
    let _ = writeln!(
        s,
        "clone: plan hashes: sha256 of the source (pre-copy) + sha256 of the destination (re-read); verification is mandatory"
    );
    if src.sectors > dst.sectors {
        let _ = writeln!(
            s,
            "clone: refusing: source {} sectors > destination {} sectors — destination is too small",
            src.sectors, dst.sectors
        );
        return false;
    }
    true
}

/// Reset the cancellation flag (the shell command does this at entry).
pub fn cancel_reset() {
    CANCEL.store(false, Ordering::Relaxed);
}

fn check_cancel() -> bool {
    if let Some(b) = crate::input::poll_byte() {
        if b == b'q' || b == b'Q' {
            CANCEL.store(true, Ordering::Relaxed);
        }
    }
    CANCEL.load(Ordering::Relaxed)
}

/// The fixed 1 MiB bounce buffer (only one pass uses it at a time).
static mut BOUNCE: [u8; BUF_BYTES] = [0; BUF_BYTES];

fn bounce() -> &'static mut [u8] {
    unsafe { core::slice::from_raw_parts_mut(core::ptr::addr_of_mut!(BOUNCE).cast::<u8>(), BUF_BYTES) }
}

fn hex_text(d: &[u8; 32]) -> [u8; 64] {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = [0u8; 64];
    for (i, b) in d.iter().enumerate() {
        out[i * 2] = HEX[(b >> 4) as usize];
        out[i * 2 + 1] = HEX[(b & 0xF) as usize];
    }
    out
}
