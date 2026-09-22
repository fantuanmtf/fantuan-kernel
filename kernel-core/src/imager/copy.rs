//! The imager's `run` orchestration: source pre-hash, buffered copy,
//! destination re-read verification and the verdict inputs for the report.
//! The streaming passes live in `pass.rs`, the policy in `policy.rs` and the
//! report writer in `report.rs`.
//!
//! Bad-sector policy (M12-3): without `--continue` an unreadable sector
//! aborts the run with the exact LBA; with `--continue` a chunk read is
//! retried, then isolated per sector, and each sector that stays unreadable
//! is counted, recorded as an LBA range and zero-filled in the output stream
//! (documented in docs/M12_TOOLS_HW.md §2b). The copy pass never re-reads a
//! sector the pre-hash already recorded as bad.

use core::fmt::Write;

use crate::log::Log;
use crate::vfs::RepairToken;

use super::pass::{copy_all, hash_range, CopyErr, HashStop};
use super::policy::{BadSectors, Options};
use super::report::Outcome;
use super::Dev;

fn hex_str<'a>(d: &'a [u8; 32], buf: &'a mut [u8; 64]) -> &'a str {
    *buf = super::hex_text(d);
    core::str::from_utf8(buf).unwrap_or("?")
}

/// Plan re-check + pre-copy hash + copy + destination re-read hash. The
/// returned outcome carries the hashes, bad-sector ledger and verdict inputs
/// for `report::emit`; `ok` means the destination verified against the
/// expected stream.
pub fn run(s: &mut Log, src: &Dev, dst: &Dev, opts: &Options, _token: &RepairToken) -> Outcome {
    if src.sectors > dst.sectors {
        let _ = writeln!(
            s,
            "clone: refusing: source {} sectors > destination {} sectors — destination is too small",
            src.sectors, dst.sectors
        );
        return Outcome::failed(BadSectors::EMPTY);
    }
    super::cancel_reset();

    let mut src_bad = BadSectors::EMPTY;
    let _ = writeln!(s, "clone: hashing source ({} sectors)...", src.sectors);
    let src_hash = match hash_range(s, src, src.sectors, opts, &mut src_bad) {
        Ok(h) => h,
        Err(HashStop::Cancelled { lba }) => {
            let _ = writeln!(s, "clone: cancelled while hashing source at LBA {} — nothing was written", lba);
            return Outcome::cancelled(src_bad);
        }
        Err(HashStop::Read { lba }) => {
            let _ = writeln!(s, "clone: read failed at LBA {} after {} retries while hashing source", lba, opts.retries);
            let _ = writeln!(s, "clone: FAILED — source is not readable; nothing was written");
            return Outcome::failed(src_bad);
        }
    };
    let mut hexbuf = [0u8; 64];
    if src_bad.total() > 0 {
        let _ = writeln!(
            s,
            "clone: source sha256 {} ({} bad sectors zero-filled)",
            hex_str(&src_hash, &mut hexbuf),
            src_bad.total()
        );
    } else {
        let _ = writeln!(s, "clone: source sha256 {}", hex_str(&src_hash, &mut hexbuf));
    }

    let _ = writeln!(s, "clone: copying (press 'q' to cancel)...");
    let (copied, stream) = match copy_all(s, src, dst, opts, &mut src_bad) {
        Ok(ok) => {
            if src_bad.total() > 0 {
                let _ = writeln!(s, "clone: wrote {} sectors ({} bad sectors zero-filled)", ok.copied, src_bad.total());
            } else {
                let _ = writeln!(s, "clone: wrote {} sectors", ok.copied);
            }
            (ok.copied, ok.stream)
        }
        Err(CopyErr::ReadOnly { lba }) => {
            let _ = writeln!(s, "clone: destination is read-only on this build (blk_write failed at LBA {})", lba);
            return Outcome::failed(src_bad);
        }
        Err(CopyErr::Write { lba }) => {
            let _ = writeln!(s, "clone: write failed at LBA {} after {} retries — destination is partially written", lba, opts.retries);
            return Outcome::failed(src_bad);
        }
        Err(CopyErr::Read { lba }) => {
            let _ = writeln!(s, "clone: read failed at LBA {} after {} retries — aborting (destination is partially written)", lba, opts.retries);
            return Outcome::failed(src_bad);
        }
        Err(CopyErr::Cancelled { lba }) => {
            let _ = writeln!(s, "clone: cancelled at LBA {} — destination is partially written and NOT verified", lba);
            return Outcome::cancelled(src_bad);
        }
    };

    let _ = writeln!(s, "clone: verifying destination (re-read)...");
    let vopts = Options { quick: opts.quick, continue_on_error: false, retries: opts.retries };
    let mut vsink = BadSectors::EMPTY;
    let dst_hash = match hash_range(s, dst, src.sectors, &vopts, &mut vsink) {
        Ok(h) => h,
        Err(HashStop::Cancelled { lba }) => {
            let _ = writeln!(s, "clone: cancelled while hashing destination at LBA {}", lba);
            return Outcome::cancelled(src_bad);
        }
        Err(HashStop::Read { lba }) => {
            let _ = writeln!(s, "clone: destination read failed at LBA {} — the copy is NOT verified", lba);
            return Outcome::failed(src_bad);
        }
    };
    let _ = writeln!(s, "clone: destination sha256 {}", hex_str(&dst_hash, &mut hexbuf));
    if opts.continue_on_error {
        let _ = writeln!(s, "clone: stream sha256 {}", hex_str(&stream, &mut hexbuf));
    }
    let source_match = stream == src_hash;
    let dest_match = if opts.continue_on_error { dst_hash == stream } else { dst_hash == src_hash };
    let ok = if dest_match {
        if opts.continue_on_error && !source_match {
            let _ = writeln!(s, "clone: verify ok — destination matches the written stream; source changed or grew bad sectors during the copy (NOT a byte-for-byte source copy)");
        } else if opts.continue_on_error && src_bad.total() > 0 {
            let _ = writeln!(s, "clone: verify ok — destination matches the zero-filled source ({} bad sectors zero-filled)", src_bad.total());
        } else {
            let _ = writeln!(s, "clone: verify ok — source and destination hashes match");
        }
        true
    } else {
        let _ = writeln!(s, "clone: verify FAILED — destination does not match the expected stream");
        false
    };
    let partial = src_bad.total() > 0 || (opts.continue_on_error && !source_match);
    Outcome {
        ok,
        partial,
        quick: opts.quick,
        copied,
        src_hash: Some(src_hash),
        stream_hash: Some(stream),
        dst_hash: Some(dst_hash),
        source_match,
        bad: src_bad,
        cancelled: false,
    }
}
