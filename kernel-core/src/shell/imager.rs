//! `clone` — verified raw disk copy (CONFIG_IMAGER). Policy layer for the
//! shared imager core: parse `clone <src> <dst> [--quick] [--continue]
//! [--retries N] [--verify] [--yes]`, print the plan, enforce the size gate,
//! obtain consent (YES) and only then enable repair mode and run the
//! hash-verified copy. Like `grub-fix`, the prompt reads the next shell/script
//! line, so the YES gate is scriptable.
//!
//! Every run ends with a deterministic report written to `/tmp/clone-report.txt`
//! (tmpfs) and mirrored to the serial log (`imager::report`).

use core::fmt::Write;

use crate::imager;
use crate::log::Log;

use super::Shell;

macro_rules! out {
    ($s:expr, $($arg:tt)*) => {
        { let _ = writeln!($s, $($arg)*); }
    };
}

fn usage(s: &mut Log) {
    out!(s, "usage: clone <src> <dst> [--quick] [--continue] [--retries N] [--verify] [--yes] — raw devices blk0..blk3");
    out!(s, "  --quick     hash/verify the first + last 1 MiB and 25/50/75% samples only");
    out!(s, "  --continue  retry, zero-fill and record unreadable sectors, then carry on");
    out!(s, "  --retries N read retries per sector before it counts as bad (default 3, max 16)");
    out!(s, "  --verify    accepted explicitly; hash verification is mandatory");
    out!(s, "  --yes       skip the interactive YES confirmation");
}

fn parse_u32(d: &[u8]) -> Option<u32> {
    if d.is_empty() {
        return None;
    }
    let mut v = 0u32;
    for &c in d {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v.saturating_mul(10).saturating_add((c - b'0') as u32);
    }
    Some(v)
}

pub fn cmd_clone(sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    let mut yes = false;
    let mut opts = imager::Options::default();
    let mut paths: [&[u8]; 2] = [&[]; 2];
    let mut n = 0usize;
    let mut i = 0usize;
    while i < args.len() {
        let a = args[i];
        if a == b"--yes" {
            yes = true;
        } else if a == b"--verify" {
            // verification always runs; the flag is the documented default
        } else if a == b"--quick" {
            opts.quick = true;
        } else if a == b"--continue" {
            opts.continue_on_error = true;
        } else if a == b"--retries" {
            i += 1;
            let Some(v) = args.get(i).and_then(|v| parse_u32(v)) else {
                out!(s, "clone: --retries needs a count (1..=16)");
                usage(s);
                return;
            };
            opts.retries = v.clamp(1, 16);
        } else if a.starts_with(b"-") {
            out!(s, "clone: unsupported option");
            usage(s);
            return;
        } else if n < 2 {
            paths[n] = a;
            n += 1;
        } else {
            usage(s);
            return;
        }
        i += 1;
    }
    if n != 2 {
        usage(s);
        return;
    }

    let (Some(si), Some(di)) = (imager::parse_path(paths[0]), imager::parse_path(paths[1])) else {
        out!(s, "clone: source and destination must be raw block devices (blk0..blk3)");
        return;
    };
    if si == di {
        out!(s, "clone: source and destination are the same device (blk{}) — refusing", si);
        return;
    }
    let (Some(src), Some(dst)) = (imager::open(si), imager::open(di)) else {
        out!(s, "clone: device not present or identity unreadable (blk{} -> blk{})", si, di);
        return;
    };
    if !imager::plan(s, &src, &dst, &opts) {
        return;
    }

    if !yes {
        out!(
            s,
            "WARNING: clone overwrites every sector of blk{} ({}) with blk{} — confirm by typing YES",
            di,
            dst.name,
            si
        );
        let _ = write!(s, "confirm> ");
        if !sh.read_line(s) || sh.line_bytes() != b"YES" {
            out!(s, "clone: confirmation not YES — aborted (nothing written)");
            return;
        }
    }

    crate::vfs::enable_repair_mode();
    let Some(token) = crate::vfs::repair_guard() else {
        out!(s, "clone: repair mode unavailable — aborted (nothing written)");
        return;
    };
    out!(s, "clone: repair mode ON — starting verified copy (press 'q' to cancel)");
    let outcome = imager::run(s, &src, &dst, &opts, &token);
    if outcome.ok {
        if outcome.partial {
            out!(
                s,
                "clone: done ({} sectors copied; {} bad sector(s) zero-filled; PARTIAL — not a byte-for-byte source copy)",
                outcome.copied,
                outcome.bad.total()
            );
        } else if outcome.quick {
            out!(s, "clone: done ({} sectors copied and sha256-verified, quick sample)", outcome.copied);
        } else {
            out!(s, "clone: done ({} sectors copied and sha256-verified)", outcome.copied);
        }
    } else {
        out!(s, "clone: FAILED — the destination is not a verified copy");
    }
    imager::report::emit(s, &src, &dst, &opts, &outcome);
}
