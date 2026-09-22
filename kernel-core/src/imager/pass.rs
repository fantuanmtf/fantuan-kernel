//! Imager low-level passes: retrying chunk/sector I/O, the bad-sector policy
//! fill, the streaming hash and the buffered copy. `copy.rs` orchestrates
//! them; `policy.rs` owns the options and the ledger. Split out to keep every
//! file inside the size rule.

use core::ffi::c_void;
use core::fmt::Write;

use crate::log::Log;
use crate::sha256::Sha256;

use super::policy::{quick_has_chunk, BadSectors, Options};
use super::{bounce, check_cancel, Dev, SECTOR};

pub(super) enum CopyErr {
    Read { lba: u64 },
    Write { lba: u64 },
    ReadOnly { lba: u64 },
    Cancelled { lba: u64 },
}

pub(super) enum HashStop {
    Read { lba: u64 },
    Cancelled { lba: u64 },
}

pub(super) struct CopyOk {
    pub copied: u64,
    pub stream: [u8; 32],
}

fn write_chunk(dev: &Dev, lba: u64, nsectors: usize, buf: &[u8], s: &mut Log, opts: &Options) -> bool {
    let mut attempt = 0u32;
    loop {
        let rc = unsafe { super::blk_write(dev.handle, lba, buf.as_ptr() as *const c_void, nsectors) };
        if rc == 0 {
            return true;
        }
        if attempt >= opts.retries {
            return false;
        }
        attempt += 1;
        let _ = writeln!(s, "clone: write retry {}/{} at LBA {}", attempt, opts.retries, lba);
    }
}

/// One sector with `opts.retries` retries; true on success (data in `buf`).
fn read_sector(s: &mut Log, dev: &Dev, lba: u64, buf: &mut [u8], opts: &Options) -> bool {
    let mut attempt = 0u32;
    loop {
        let rc = unsafe { super::blk_read(dev.handle, lba, buf.as_mut_ptr() as *mut c_void, 1) };
        if rc == 0 {
            return true;
        }
        if attempt >= opts.retries {
            return false;
        }
        attempt += 1;
        let _ = writeln!(s, "clone: read retry {}/{} at LBA {}", attempt, opts.retries, lba);
    }
}

/// Per-sector fallback for an unreadable chunk: successful sectors are kept,
/// already-recorded bad sectors are zero-filled, new failures are recorded
/// with `--continue` or returned as the abort LBA without it.
fn fill_per_sector(
    s: &mut Log,
    dev: &Dev,
    lba: u64,
    n: usize,
    buf: &mut [u8],
    opts: &Options,
    bad: &mut BadSectors,
) -> Result<(), u64> {
    for i in 0..n {
        let slba = lba + i as u64;
        let sec = &mut buf[i * SECTOR as usize..(i + 1) * SECTOR as usize];
        if bad.contains(slba) {
            sec.fill(0);
            continue;
        }
        if read_sector(s, dev, slba, sec, opts) {
            continue;
        }
        bad.record(slba, opts.retries);
        if !opts.continue_on_error {
            return Err(slba);
        }
        let _ = writeln!(s, "clone: bad sector LBA {} after {} retries — zero-filled", slba, opts.retries);
        sec.fill(0);
    }
    Ok(())
}

/// Fill `buf[..n*512]` for `n` sectors at `lba` under the bad-sector policy.
/// Err(lba) is the exact unreadable sector when `--continue` is off.
pub(super) fn fill_chunk(
    s: &mut Log,
    dev: &Dev,
    lba: u64,
    n: usize,
    buf: &mut [u8],
    opts: &Options,
    bad: &mut BadSectors,
) -> Result<(), u64> {
    if bad.any_in(lba, n as u64) {
        return fill_per_sector(s, dev, lba, n, buf, opts, bad);
    }
    let mut attempt = 0u32;
    loop {
        let rc = unsafe { super::blk_read(dev.handle, lba, buf.as_mut_ptr() as *mut c_void, n) };
        if rc == 0 {
            return Ok(());
        }
        if attempt >= opts.retries {
            break;
        }
        attempt += 1;
        let _ = writeln!(s, "clone: read retry {}/{} at LBA {}", attempt, opts.retries, lba);
    }
    let _ = writeln!(s, "clone: chunk at LBA {} unreadable after {} retries — isolating sectors", lba, opts.retries);
    fill_per_sector(s, dev, lba, n, buf, opts, bad)
}

/// Stream-hash `total` sectors of DEV. `--quick` reads/hashes only the
/// sampled windows; bad sectors are zero-filled in the hash under
/// `--continue` (that hash is the "zero-filled source expectation").
pub(super) fn hash_range(
    s: &mut Log,
    dev: &Dev,
    total: u64,
    opts: &Options,
    bad: &mut BadSectors,
) -> Result<[u8; 32], HashStop> {
    let mut h = Sha256::new();
    let buf = bounce();
    let per_chunk = buf.len() / SECTOR as usize;
    let mut lba = 0u64;
    while lba < total {
        if check_cancel() {
            return Err(HashStop::Cancelled { lba });
        }
        let n = ((total - lba) as usize).min(per_chunk);
        let bytes = n * SECTOR as usize;
        if !opts.quick || quick_has_chunk(lba, n as u64, total) {
            if let Err(bl) = fill_chunk(s, dev, lba, n, &mut buf[..bytes], opts, bad) {
                return Err(HashStop::Read { lba: bl });
            }
            h.update(&buf[..bytes]);
        }
        lba += n as u64;
    }
    Ok(h.finish())
}

/// Sector copy with progress every 5%, cancellation on 'q', bounded retries
/// and the bad-sector policy. The stream hash covers exactly the bytes that
/// were written (sampled under `--quick`). The destination's first-block
/// failure is reported as a read-only destination (the i686 build's
/// `blk_write` stub always returns -1).
pub(super) fn copy_all(
    s: &mut Log,
    src: &Dev,
    dst: &Dev,
    opts: &Options,
    bad: &mut BadSectors,
) -> Result<CopyOk, CopyErr> {
    let total = src.sectors;
    let buf = bounce();
    let per_chunk = buf.len() / SECTOR as usize;
    let mut lba = 0u64;
    let mut next_pct = 5u64;
    let mut h = Sha256::new();
    while lba < total {
        if check_cancel() {
            return Err(CopyErr::Cancelled { lba });
        }
        let n = ((total - lba) as usize).min(per_chunk);
        let bytes = n * SECTOR as usize;
        if let Err(bl) = fill_chunk(s, src, lba, n, &mut buf[..bytes], opts, bad) {
            return Err(CopyErr::Read { lba: bl });
        }
        if !opts.quick || quick_has_chunk(lba, n as u64, total) {
            h.update(&buf[..bytes]);
        }
        if !write_chunk(dst, lba, n, &buf[..bytes], s, opts) {
            return if lba == 0 { Err(CopyErr::ReadOnly { lba }) } else { Err(CopyErr::Write { lba }) };
        }
        lba += n as u64;
        let pct = lba * 100 / total.max(1);
        if pct >= next_pct {
            let _ = writeln!(s, "clone: {}% ({}/{} sectors)", next_pct, lba, total);
            next_pct = (pct / 5 + 1) * 5;
        }
    }
    Ok(CopyOk { copied: lba, stream: h.finish() })
}
