//! The imager's streaming passes: source pre-hash, the buffered sector copy
//! (progress every 5%, 'q' cancels) and the destination re-read hash. The
//! public entry point is `run`; `mod.rs` owns the device/plan layer.

use core::ffi::c_void;
use core::fmt::Write;

use crate::log::Log;
use crate::sha256::Sha256;
use crate::vfs::RepairToken;

use super::{bounce, check_cancel, hex_text, Dev, RETRIES, SECTOR};

enum CopyErr {
    Read { lba: u64 },
    Write { lba: u64 },
    ReadOnly { lba: u64 },
    Cancelled { lba: u64 },
}

fn read_chunk(dev: &Dev, lba: u64, nsectors: usize, buf: &mut [u8], s: &mut Log) -> bool {
    let mut attempt = 0u32;
    loop {
        let rc = unsafe { super::blk_read(dev.handle, lba, buf.as_mut_ptr() as *mut c_void, nsectors) };
        if rc == 0 {
            return true;
        }
        if attempt >= RETRIES {
            return false;
        }
        attempt += 1;
        let _ = writeln!(s, "clone: read retry {}/{} at LBA {}", attempt, RETRIES, lba);
    }
}

fn write_chunk(dev: &Dev, lba: u64, nsectors: usize, buf: &[u8], s: &mut Log) -> bool {
    let mut attempt = 0u32;
    loop {
        let rc = unsafe { super::blk_write(dev.handle, lba, buf.as_ptr() as *const c_void, nsectors) };
        if rc == 0 {
            return true;
        }
        if attempt >= RETRIES {
            return false;
        }
        attempt += 1;
        let _ = writeln!(s, "clone: write retry {}/{} at LBA {}", attempt, RETRIES, lba);
    }
}

/// Streaming hash of SECTORS sectors of DEV (the destination is hashed over
/// the copied range only; its larger tail is documented as untouched). None
/// on read failure/cancel (the specific reason is logged here).
fn hash_all(s: &mut Log, dev: &Dev, label: &str, sectors: u64) -> Option<[u8; 32]> {
    let mut h = Sha256::new();
    let buf = bounce();
    let per_chunk = buf.len() / SECTOR as usize;
    let mut lba = 0u64;
    while lba < sectors {
        if check_cancel() {
            let _ = writeln!(s, "clone: cancelled while hashing {} at LBA {}", label, lba);
            return None;
        }
        let n = ((sectors - lba) as usize).min(per_chunk);
        let bytes = n * SECTOR as usize;
        if !read_chunk(dev, lba, n, &mut buf[..bytes], s) {
            let _ = writeln!(s, "clone: read failed at LBA {} after {} retries while hashing {}", lba, RETRIES, label);
            return None;
        }
        h.update(&buf[..bytes]);
        lba += n as u64;
    }
    Some(h.finish())
}

/// Sector copy with progress every 5%, cancellation on 'q', and bounded
/// retries. The destination's first-block failure is reported as a
/// read-only destination (the i686 build's blk_write stub always returns -1).
fn copy_all(s: &mut Log, src: &Dev, dst: &Dev) -> Result<u64, CopyErr> {
    let total = src.sectors;
    let buf = bounce();
    let per_chunk = buf.len() / SECTOR as usize;
    let mut lba = 0u64;
    let mut next_pct = 5u64;
    while lba < total {
        if check_cancel() {
            return Err(CopyErr::Cancelled { lba });
        }
        let n = ((total - lba) as usize).min(per_chunk);
        let bytes = n * SECTOR as usize;
        if !read_chunk(src, lba, n, &mut buf[..bytes], s) {
            return Err(CopyErr::Read { lba });
        }
        if !write_chunk(dst, lba, n, &buf[..bytes], s) {
            return if lba == 0 { Err(CopyErr::ReadOnly { lba }) } else { Err(CopyErr::Write { lba }) };
        }
        lba += n as u64;
        let pct = lba * 100 / total.max(1);
        if pct >= next_pct {
            let _ = writeln!(s, "clone: {}% ({}/{} sectors)", next_pct, lba, total);
            next_pct = (pct / 5 + 1) * 5;
        }
    }
    Ok(lba)
}

/// Plan re-check + pre-copy hash + copy + destination re-read hash. True only
/// when the destination verifies against the source.
pub fn run(s: &mut Log, src: &Dev, dst: &Dev, _token: &RepairToken) -> bool {
    if src.sectors > dst.sectors {
        let _ = writeln!(
            s,
            "clone: refusing: source {} sectors > destination {} sectors — destination is too small",
            src.sectors, dst.sectors
        );
        return false;
    }
    super::cancel_reset();

    let _ = writeln!(s, "clone: hashing source ({} sectors)...", src.sectors);
    let Some(src_hash) = hash_all(s, src, "source", src.sectors) else {
        let _ = writeln!(s, "clone: FAILED — source is not readable; nothing was written");
        return false;
    };
    let src_hex = hex_text(&src_hash);
    let _ = writeln!(s, "clone: source sha256 {}", core::str::from_utf8(&src_hex).unwrap_or("?"));

    let _ = writeln!(s, "clone: copying (press 'q' to cancel)...");
    match copy_all(s, src, dst) {
        Ok(n) => {
            let _ = writeln!(s, "clone: wrote {} sectors", n);
        }
        Err(CopyErr::ReadOnly { lba }) => {
            let _ = writeln!(
                s,
                "clone: destination is read-only on this build (blk_write failed at LBA {})",
                lba
            );
            return false;
        }
        Err(CopyErr::Write { lba }) => {
            let _ = writeln!(s, "clone: write failed at LBA {} after {} retries — destination is partially written", lba, RETRIES);
            return false;
        }
        Err(CopyErr::Read { lba }) => {
            let _ = writeln!(s, "clone: read failed at LBA {} after {} retries — aborting (destination is partially written)", lba, RETRIES);
            return false;
        }
        Err(CopyErr::Cancelled { lba }) => {
            let _ = writeln!(s, "clone: cancelled at LBA {} — destination is partially written and NOT verified", lba);
            return false;
        }
    }

    let _ = writeln!(s, "clone: verifying destination (re-read)...");
    let Some(dst_hash) = hash_all(s, dst, "destination", src.sectors) else {
        let _ = writeln!(s, "clone: FAILED — destination is not readable; the copy is NOT verified");
        return false;
    };
    let dst_hex = hex_text(&dst_hash);
    let _ = writeln!(s, "clone: destination sha256 {}", core::str::from_utf8(&dst_hex).unwrap_or("?"));
    if src_hash == dst_hash {
        let _ = writeln!(s, "clone: verify ok — source and destination hashes match");
        true
    } else {
        let _ = writeln!(
            s,
            "clone: verify FAILED — source sha256 {} destination sha256 {}",
            core::str::from_utf8(&src_hex).unwrap_or("?"),
            core::str::from_utf8(&dst_hex).unwrap_or("?")
        );
        false
    }
}
