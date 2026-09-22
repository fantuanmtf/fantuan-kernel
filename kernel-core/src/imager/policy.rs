//! Imager copy policy (M12-3): the `--quick`/`--continue` options, the
//! bad-sector ledger and the quick-verification windows. Split from `copy.rs`
//! and `mod.rs` to keep every file inside the size rule.
//!
//! The ledger is fixed-capacity (no heap): up to `MAX_BAD_RANGES` unreadable
//! ranges are tracked with per-range error/retry counts; anything beyond is
//! still counted in the totals and flagged as overflow.

use super::SECTOR;

/// Copy policy from the shell; the default reproduces M12-2 behavior.
#[derive(Clone, Copy)]
pub struct Options {
    /// `--quick`: hash/verify sampled 1 MiB windows, not the whole range.
    pub quick: bool,
    /// `--continue`: retry, zero-fill and record unreadable sectors.
    pub continue_on_error: bool,
    /// Read retries per sector before it counts as bad (`--retries N`).
    pub retries: u32,
}

impl Default for Options {
    fn default() -> Self {
        Options { quick: false, continue_on_error: false, retries: super::RETRIES }
    }
}

pub const MAX_BAD_RANGES: usize = 16;

/// One coalesced run of unreadable sectors.
#[derive(Clone, Copy)]
pub struct BadRange {
    pub lba: u64,
    pub count: u64,
    /// Unreadable sectors in this range (the final failures).
    pub errors: u32,
    /// Retry attempts spent on them.
    pub retries: u32,
}

const EMPTY_RANGE: BadRange = BadRange { lba: 0, count: 0, errors: 0, retries: 0 };

/// Fixed-capacity bad-sector ledger shared by the hash and copy passes.
#[derive(Clone, Copy)]
pub struct BadSectors {
    pub ranges: [BadRange; MAX_BAD_RANGES],
    pub count: usize,
    /// Ranges that did not fit (totals cover them; the list does not).
    pub overflow: usize,
    pub errors: u32,
    pub retries: u32,
}

impl BadSectors {
    pub const EMPTY: BadSectors =
        BadSectors { ranges: [EMPTY_RANGE; MAX_BAD_RANGES], count: 0, overflow: 0, errors: 0, retries: 0 };

    /// Record one unreadable sector (scans run in ascending LBA order, so
    /// adjacent sectors coalesce into one range).
    pub fn record(&mut self, lba: u64, retries: u32) {
        self.errors += 1;
        self.retries += retries;
        if let Some(r) = self.ranges[..self.count].iter_mut().find(|r| r.lba + r.count == lba) {
            r.count += 1;
            r.errors += 1;
            r.retries += retries;
            return;
        }
        if self.count < MAX_BAD_RANGES {
            self.ranges[self.count] = BadRange { lba, count: 1, errors: 1, retries };
            self.count += 1;
        } else {
            self.overflow += 1;
        }
    }

    pub fn contains(&self, lba: u64) -> bool {
        self.ranges[..self.count].iter().any(|r| lba >= r.lba && lba < r.lba + r.count)
    }

    /// True when any recorded range intersects `[lba, lba + n)`.
    pub fn any_in(&self, lba: u64, n: u64) -> bool {
        self.ranges[..self.count].iter().any(|r| lba < r.lba + r.count && r.lba < lba + n)
    }

    pub fn total(&self) -> u64 {
        self.ranges[..self.count].iter().map(|r| r.count).sum()
    }
}

/// One quick-sample window: 1 MiB in sectors.
const WINDOW: u64 = (1 << 20) / SECTOR;

/// True when chunk `[lba, lba+n)` belongs to the `--quick` sample: the first
/// and last 1 MiB plus 1 MiB at 25/50/75%, aligned down to a 1 MiB boundary.
/// Deterministic and chunk-aligned; the M12 design's "first/last 1 MiB plus a
/// strided sample".
pub fn quick_has_chunk(lba: u64, n: u64, total: u64) -> bool {
    if n == 0 {
        return false;
    }
    let hit = |start: u64| lba < start + WINDOW && start < lba + n;
    if hit(0) || (total > WINDOW && hit(total - WINDOW)) {
        return true;
    }
    for pct in [25u64, 50, 75] {
        let start = (total / 100 * pct) & !(WINDOW - 1);
        if start < total && hit(start) {
            return true;
        }
    }
    false
}
