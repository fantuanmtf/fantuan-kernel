//! Deterministic clone report (M12-3). The report is built into a fixed
//! buffer (no heap), mirrored to the serial log in one write and written to
//! the tmpfs at `/tmp/clone-report.txt`, so a smoke can read it from the
//! transcript and the operator can inspect it in the running system.
//!
//! Format (stable, one field per line, `clone-report:` prefixed):
//!   clone-report v1
//!   clone-report: source blk1 ahci 2048 sectors 1048576 bytes
//!   clone-report: policy continue=yes quick=no retries=3
//!   clone-report: verify full|quick
//!   clone-report: source-sha256 <hex> / stream-sha256 / destination-sha256
//!   clone-report: bad-range lba=100 count=4 errors=4 retries=12
//!   clone-report: bad-ranges 2 errors 6 retries 18
//!   clone-report: source-match yes
//!   clone-report: verdict partial
//!   clone-report: end

use core::fmt::{self, Write};

use crate::log::Log;

use super::policy::{BadSectors, Options};
use super::{hex_text, Dev, SECTOR};

/// tmpfs location of the report (documented in docs/USAGE.md §4).
pub const REPORT_PATH: &str = "/tmp/clone-report.txt";
const REPORT_DIR: &[u8] = b"/tmp";
const REPORT_NAME: &[u8] = b"clone-report.txt";

/// Verdict inputs produced by `copy::run`.
pub struct Outcome {
    /// The destination verified against the expected stream.
    pub ok: bool,
    /// Bad sectors were zero-filled (or the source changed mid-copy).
    pub partial: bool,
    pub quick: bool,
    pub copied: u64,
    pub src_hash: Option<[u8; 32]>,
    /// Hash of the bytes actually written (zero-fill included).
    pub stream_hash: Option<[u8; 32]>,
    pub dst_hash: Option<[u8; 32]>,
    pub source_match: bool,
    pub bad: BadSectors,
    pub cancelled: bool,
}

impl Outcome {
    pub const fn failed(bad: BadSectors) -> Outcome {
        Outcome {
            ok: false,
            partial: false,
            quick: false,
            copied: 0,
            src_hash: None,
            stream_hash: None,
            dst_hash: None,
            source_match: false,
            bad,
            cancelled: false,
        }
    }

    pub const fn cancelled(bad: BadSectors) -> Outcome {
        let mut o = Outcome::failed(bad);
        o.cancelled = true;
        o
    }

    pub fn verdict(&self) -> &'static str {
        if self.cancelled {
            "cancelled"
        } else if !self.ok {
            "failed"
        } else if self.partial || !self.source_match {
            "partial"
        } else {
            "verified"
        }
    }
}

/// Fixed report buffer: 2 KiB is far above the ~12 lines the format needs.
struct Buf {
    b: [u8; 2048],
    n: usize,
}

impl fmt::Write for Buf {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        if self.n + bytes.len() > self.b.len() {
            return Err(fmt::Error);
        }
        self.b[self.n..self.n + bytes.len()].copy_from_slice(bytes);
        self.n += bytes.len();
        Ok(())
    }
}

fn hash_field(tag: &str, h: &Option<[u8; 32]>, r: &mut Buf) {
    let _ = write!(r, "clone-report: {tag} ");
    match h {
        Some(d) => {
            let _ = r.write_str(core::str::from_utf8(&hex_text(d)).unwrap_or("?"));
        }
        None => {
            let _ = r.write_str("none");
        }
    }
    let _ = r.write_str("\n");
}

/// Format OUT into a buffer, write it to the tmpfs and mirror it to serial.
pub fn emit(s: &mut Log, src: &Dev, dst: &Dev, opts: &Options, out: &Outcome) {
    let mut r = Buf { b: [0; 2048], n: 0 };
    let yesno = |v: bool| if v { "yes" } else { "no" };
    let _ = writeln!(r, "clone-report v1");
    let _ = writeln!(
        r,
        "clone-report: source blk{} {} {} sectors {} bytes",
        src.index,
        src.name,
        src.sectors,
        src.sectors * SECTOR
    );
    let _ = writeln!(
        r,
        "clone-report: destination blk{} {} {} sectors {} bytes",
        dst.index,
        dst.name,
        dst.sectors,
        dst.sectors * SECTOR
    );
    let _ = writeln!(r, "clone-report: sector-size {}", SECTOR);
    let _ = writeln!(
        r,
        "clone-report: policy continue={} quick={} retries={}",
        yesno(opts.continue_on_error),
        yesno(opts.quick),
        opts.retries
    );
    let _ = writeln!(r, "clone-report: verify {}", if opts.quick { "quick" } else { "full" });
    let _ = writeln!(r, "clone-report: copied {} sectors", out.copied);
    hash_field("source-sha256", &out.src_hash, &mut r);
    hash_field("stream-sha256", &out.stream_hash, &mut r);
    hash_field("destination-sha256", &out.dst_hash, &mut r);
    let _ = writeln!(r, "clone-report: source-match {}", yesno(out.source_match));
    for range in out.bad.ranges[..out.bad.count].iter() {
        let _ = writeln!(
            r,
            "clone-report: bad-range lba={} count={} errors={} retries={}",
            range.lba, range.count, range.errors, range.retries
        );
    }
    if out.bad.overflow > 0 {
        let _ = writeln!(r, "clone-report: bad-ranges-overflow {}", out.bad.overflow);
    }
    let _ = writeln!(r, "clone-report: bad-ranges {}", out.bad.count);
    let _ = writeln!(r, "clone-report: errors {}", out.bad.errors);
    let _ = writeln!(r, "clone-report: retries {}", out.bad.retries);
    let _ = writeln!(r, "clone-report: verdict {}", out.verdict());
    let _ = writeln!(r, "clone-report: end");

    let stored = crate::vfs::io::write_mem_file(REPORT_DIR, REPORT_NAME, &r.b[..r.n]);
    let _ = writeln!(
        s,
        "clone: report -> {} ({} bytes{})",
        REPORT_PATH,
        r.n,
        if stored { "" } else { ", tmpfs write failed" }
    );
    let _ = s.write(&r.b[..r.n]);
}
