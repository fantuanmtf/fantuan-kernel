//! `clone` — verified raw disk copy (CONFIG_IMAGER). Policy layer for the
//! shared imager core: parse `clone <src> <dst> [--verify] [--yes]`, print
//! the plan, enforce the size gate, obtain consent (YES) and only then
//! enable repair mode and run the hash-verified copy. Like `grub-fix`, the
//! prompt reads the next shell/script line, so the YES gate is scriptable.

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
    out!(s, "usage: clone <src> <dst> [--verify] [--yes] — raw devices blk0..blk3");
    out!(s, "  --verify  accepted explicitly; hash verification is mandatory");
    out!(s, "  --yes     skip the interactive YES confirmation");
}

pub fn cmd_clone(sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    let mut yes = false;
    let mut paths: [&[u8]; 2] = [&[]; 2];
    let mut n = 0usize;
    for &a in args {
        if a == b"--yes" {
            yes = true;
        } else if a == b"--verify" {
            // verification always runs; the flag is the documented default
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
    if !imager::plan(s, &src, &dst) {
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
    if imager::run(s, &src, &dst, &token) {
        out!(s, "clone: done ({} sectors copied and sha256-verified)", src.sectors.min(dst.sectors));
    } else {
        out!(s, "clone: FAILED — the destination is not a verified copy");
    }
}
