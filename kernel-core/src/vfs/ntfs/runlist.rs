//! NTFS runlist decoding (M12-4): fragmented and sparse runs, bounded.
//!
//! A runlist is a packed sequence of (length, LCN delta) pairs; a zero
//! header byte ends it. A run with an offset field of zero length is
//! sparse (reads as zeroes). The decoder is strict: malformed lengths,
//! truncated fields and over-long lists are rejected so no caller can walk
//! past what it buffered.

use crate::vfs::ntfs::NtfsErr;

/// Runs buffered per attribute; longer runlists are rejected (attribute
/// lists that chain records are a v2 item).
pub const MAX_RUNS: usize = 24;

#[derive(Clone, Copy)]
pub struct Run {
    /// First VCN (cluster index within the attribute) covered by the run.
    pub vcn: u64,
    /// Clusters covered.
    pub len: u64,
    /// First LCN; None for a sparse run (reads as zeroes).
    pub lcn: Option<u64>,
}

const EMPTY_RUN: Run = Run { vcn: 0, len: 0, lcn: None };

#[derive(Clone, Copy)]
pub struct RunList {
    pub runs: [Run; MAX_RUNS],
    pub count: usize,
    /// Total clusters covered (the VCN span of the attribute).
    pub clusters: u64,
}

impl RunList {
    pub const EMPTY: RunList = RunList { runs: [EMPTY_RUN; MAX_RUNS], count: 0, clusters: 0 };

    /// Decode a packed runlist; Err(RunList) on any malformed or oversized
    /// encoding.
    pub fn decode(data: &[u8]) -> Result<RunList, NtfsErr> {
        let mut out = RunList::EMPTY;
        let mut i = 0usize;
        let mut lcn: i64 = 0;
        let mut vcn: u64 = 0;
        loop {
            let h = *data.get(i).ok_or(NtfsErr::RunList)?;
            i += 1;
            if h == 0 {
                break;
            }
            let len_bytes = (h & 0x0F) as usize;
            let off_bytes = (h >> 4) as usize;
            if len_bytes == 0 || len_bytes > 8 || off_bytes > 8 {
                return Err(NtfsErr::RunList);
            }
            let len = read_uint(data, i, len_bytes)?;
            i += len_bytes;
            if len == 0 || out.count == MAX_RUNS {
                return Err(NtfsErr::RunList);
            }
            let lcn_start = if off_bytes == 0 {
                None
            } else {
                let delta = read_int(data, i, off_bytes)?;
                i += off_bytes;
                lcn = lcn.checked_add(delta).ok_or(NtfsErr::RunList)?;
                if lcn < 0 {
                    return Err(NtfsErr::RunList);
                }
                Some(lcn as u64)
            };
            out.runs[out.count] = Run { vcn, len, lcn: lcn_start };
            out.count += 1;
            vcn = vcn.checked_add(len).ok_or(NtfsErr::RunList)?;
        }
        out.clusters = vcn;
        if out.count == 0 {
            return Err(NtfsErr::RunList);
        }
        Ok(out)
    }

    /// The run covering VCN, or Err when the VCN lies beyond the list.
    pub fn run_at(&self, vcn: u64) -> Result<Run, NtfsErr> {
        for r in self.runs[..self.count].iter() {
            if vcn >= r.vcn && vcn - r.vcn < r.len {
                return Ok(*r);
            }
        }
        Err(NtfsErr::RunList)
    }

    /// Physical LCN of VCN (None when sparse).
    pub fn lcn_of(&self, vcn: u64) -> Result<Option<u64>, NtfsErr> {
        let r = self.run_at(vcn)?;
        match r.lcn {
            None => Ok(None),
            Some(base) => base.checked_add(vcn - r.vcn).map(Some).ok_or(NtfsErr::RunList),
        }
    }
}

fn read_uint(b: &[u8], off: usize, n: usize) -> Result<u64, NtfsErr> {
    if off + n > b.len() {
        return Err(NtfsErr::RunList);
    }
    let mut v = 0u64;
    for (i, &x) in b[off..off + n].iter().enumerate() {
        v |= (x as u64) << (8 * i);
    }
    Ok(v)
}

fn read_int(b: &[u8], off: usize, n: usize) -> Result<i64, NtfsErr> {
    let v = read_uint(b, off, n)?;
    let bits = n * 8;
    if bits == 64 {
        return Ok(v as i64);
    }
    let sign = 1u64 << (bits - 1);
    Ok(if v & sign != 0 { (v as i64) - (1i64 << bits) } else { v as i64 })
}
