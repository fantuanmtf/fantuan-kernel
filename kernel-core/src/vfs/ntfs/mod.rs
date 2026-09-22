//! NTFS read-only driver (M12-4/M12-5, M12_TOOLS_HW.md §3): boot sector/
//! BPB validation, the $MFT bootstrap, FILE records with update-sequence
//! fixups, attributes ($STANDARD_INFORMATION/$FILE_NAME/$DATA resident and
//! non-resident), runlist decoding and $I30 directory enumeration. Mounted
//! read-only at `/mnt/win0`; there is no write entry point in this module.
//!
//! v1 limits (documented): 512-byte sectors, clusters <= 4 KiB, MFT records
//! <= 1 KiB, runlists <= 24 runs, attribute lists spanning MFT records,
//! compression and encryption are rejected with a clear error. Sparse runs
//! read as zeroes.
//!
//! Module map: runlist.rs (packed runs), record.rs (FILE records/attrs),
//! index.rs ($I30 B-tree), file.rs ($DATA reads), cache.rs (2 x 4 KiB slots).

mod api;
mod cache;
mod file;
mod index;
mod record;
mod runlist;

pub use api::{
    dir_entry, mount_first, mounted, read_record_at, set_mount, stat_path, stat_record, win_rel,
};
pub use record::{AttrValue, Record};

use core::ffi::c_void;
use core::fmt::Write;

use crate::log::Log;

extern "C" {
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
}

/// The largest index block and cluster this driver buffers (one scratch
/// buffer, passed down by the callers).
pub const MAX_INDEX_BLOCK: usize = 4096;
const ROOT_RECORD: u64 = 5;
const VOLUME_RECORD: u64 = 3;

#[derive(Clone, Copy, PartialEq)]
pub enum NtfsErr {
    Io,
    Boot,
    Mft,
    Fixup,
    Record,
    RunList,
    AttrList,
    Unsupported,
    Corrupt,
    NotFound,
    NotDir,
}

impl NtfsErr {
    /// Human text; stable enough for the smoke transcripts.
    pub fn text(self) -> &'static str {
        match self {
            NtfsErr::Io => "device read failed",
            NtfsErr::Boot => "invalid NTFS boot sector",
            NtfsErr::Mft => "invalid $MFT",
            NtfsErr::Fixup => "record corrupt (update sequence mismatch)",
            NtfsErr::Record => "corrupt record/attribute",
            NtfsErr::RunList => "corrupt runlist",
            NtfsErr::AttrList => "$ATTRIBUTE_LIST spans MFT records (unsupported in v1)",
            NtfsErr::Unsupported => "unsupported NTFS feature (geometry/compression/encryption)",
            NtfsErr::Corrupt => "corrupt structure",
            NtfsErr::NotFound => "not found",
            NtfsErr::NotDir => "not a directory",
        }
    }
}

/// A mounted NTFS volume (read-only).
#[derive(Clone, Copy)]
pub struct Ntfs {
    pub part_lba: u64,
    pub sectors_per_cluster: u32,
    pub cluster_size: u32,
    pub total_clusters: u64,
    pub mft_lcn: u64,
    pub mft_record_size: u32,
    pub index_block_size: u32,
    pub volume_serial: u64,
    mft_runs: runlist::RunList,
    label: [u8; 32],
    label_len: usize,
}

impl Ntfs {
    /// Validate the boot sector and bootstrap $MFT (record 0's $DATA).
    pub fn parse(part_lba: u64) -> Result<Ntfs, NtfsErr> {
        let mut sec = [0u8; 512];
        if !read_sectors(part_lba, &mut sec) {
            return Err(NtfsErr::Io);
        }
        if &sec[3..11] != b"NTFS    " {
            return Err(NtfsErr::Boot);
        }
        if u16::from_le_bytes([sec[0x0B], sec[0x0C]]) != 512 {
            return Err(NtfsErr::Unsupported); // v1 scope: 512-byte sectors
        }
        let spc = sec[0x0D] as u32;
        if spc == 0 || spc > 128 || spc & (spc - 1) != 0 {
            return Err(NtfsErr::Boot);
        }
        let cluster_size = 512 * spc;
        if cluster_size > cache::MAX_CLUSTER as u32 {
            return Err(NtfsErr::Unsupported);
        }
        let total_sectors = record::le64(&sec, 0x28);
        let total_clusters = total_sectors / spc as u64;
        let mft_lcn = record::le64(&sec, 0x30);
        if total_clusters == 0 || mft_lcn >= total_clusters {
            return Err(NtfsErr::Boot);
        }
        let mft_record_size = record_size(sec[0x40], cluster_size)?;
        if mft_record_size > record::MAX_RECORD as u32 {
            return Err(NtfsErr::Unsupported);
        }
        let index_block_size = record_size(sec[0x44], cluster_size)?;
        if index_block_size > MAX_INDEX_BLOCK as u32 {
            return Err(NtfsErr::Unsupported);
        }

        let mut raw = [0u8; record::MAX_RECORD];
        let rec_size = mft_record_size as usize;
        let lba = part_lba + mft_lcn * spc as u64;
        if !read_sectors(lba, &mut raw[..rec_size]) {
            return Err(NtfsErr::Io);
        }
        let rec0 = Record::parse(0, &raw[..rec_size], rec_size).map_err(|_| NtfsErr::Mft)?;
        let Some(AttrValue::NonResident(nr)) = rec0.find(record::ATTR_DATA, false).map_err(|_| NtfsErr::Mft)? else {
            return Err(NtfsErr::Mft);
        };
        if nr.runs.count == 0 || nr.runs.runs[0].lcn != Some(mft_lcn) {
            return Err(NtfsErr::Mft);
        }

        let mut n = Ntfs {
            part_lba,
            sectors_per_cluster: spc,
            cluster_size,
            total_clusters,
            mft_lcn,
            mft_record_size,
            index_block_size,
            volume_serial: record::le64(&sec, 0x48),
            mft_runs: nr.runs,
            label: [0; 32],
            label_len: 0,
        };
        // Fail early on a corrupt root; read the label when $Volume is sane.
        n.read_record(ROOT_RECORD)?;
        if let Ok(rec) = n.read_record(VOLUME_RECORD) {
            if let Ok(Some(AttrValue::Resident(v))) = rec.find(record::ATTR_VOLUME_NAME, false) {
                n.label_len = utf16_to_utf8(v, &mut n.label).unwrap_or(0);
            }
        }
        Ok(n)
    }

    /// Read one MFT record (fixups applied).
    pub fn read_record(&self, number: u64) -> Result<Record, NtfsErr> {
        let off = number.checked_mul(self.mft_record_size as u64).ok_or(NtfsErr::Record)?;
        let size = self.mft_record_size as usize;
        let mut raw = [0u8; record::MAX_RECORD];
        if self.read_runs_at(&self.mft_runs, off, &mut raw[..size])? != size {
            return Err(NtfsErr::Io);
        }
        Record::parse(number, &raw[..size], size)
    }

    /// One-line volume facts for the boot log and `lsmnt`.
    pub fn describe(&self, s: &mut Log) {
        let label = core::str::from_utf8(&self.label[..self.label_len]).unwrap_or("?");
        let _ = writeln!(
            s,
            "ntfs: mounted ro — label '{}', {} clusters of {} B, MFT record {} B at LCN {}",
            label, self.total_clusters, self.cluster_size, self.mft_record_size, self.mft_lcn
        );
    }

    pub fn label(&self) -> &[u8] {
        &self.label[..self.label_len]
    }
}

/// Cheap OEM-id probe so non-NTFS partitions are not logged as rejects.
pub fn looks_like(part_lba: u64) -> bool {
    let mut sec = [0u8; 512];
    read_sectors(part_lba, &mut sec) && &sec[3..11] == b"NTFS    "
}

fn record_size(byte: u8, cluster_size: u32) -> Result<u32, NtfsErr> {    let b = byte as i8;
    let size = if b > 0 {
        (b as u32).checked_mul(cluster_size).ok_or(NtfsErr::Boot)?
    } else {
        1u32.checked_shl((-(b as i32)) as u32).ok_or(NtfsErr::Boot)?
    };
    if size < 512 || size & (size - 1) != 0 {
        return Err(NtfsErr::Boot);
    }
    Ok(size)
}

fn read_sectors(lba: u64, out: &mut [u8]) -> bool {
    if out.is_empty() || out.len() % 512 != 0 {
        return false;
    }
    unsafe { blk_read(core::ptr::null_mut(), lba, out.as_mut_ptr() as *mut c_void, out.len() / 512) == 0 }
}

/// A name is listed when it is not a DOS alias, not the root's self entry
/// and not an NTFS metadata name; that matches what a read-only viewer
/// shows (`ntfsls` hides the same set).
fn visible(name: &[u8], namespace: u8) -> bool {
    namespace != 2 && name != b"." && !matches!(name.first(), Some(b'$')) && !name.is_empty()
}

/// ASCII-case-insensitive name comparison; exact beyond ASCII.
fn name_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

/// UTF-16LE to UTF-8 into OUT; None on a bad sequence or a short buffer.
fn utf16_to_utf8(src: &[u8], out: &mut [u8]) -> Option<usize> {
    if src.len() % 2 != 0 {
        return None;
    }
    let mut o = 0usize;
    let mut i = 0usize;
    while i < src.len() {
        let u = u16::from_le_bytes([src[i], src[i + 1]]);
        i += 2;
        let cp = if (0xD800..0xDC00).contains(&u) {
            if i + 1 >= src.len() {
                return None;
            }
            let lo = u16::from_le_bytes([src[i], src[i + 1]]);
            if !(0xDC00..0xE000).contains(&lo) {
                return None;
            }
            i += 2;
            0x10000 + (((u as u32 - 0xD800) << 10) | (lo as u32 - 0xDC00))
        } else if (0xDC00..0xE000).contains(&u) {
            return None;
        } else {
            u as u32
        };
        let n = if cp < 0x80 { 1 } else if cp < 0x800 { 2 } else if cp < 0x10000 { 3 } else { 4 };
        if o + n > out.len() {
            return None;
        }
        if n == 1 {
            out[o] = cp as u8;
        } else if n == 2 {
            out[o] = 0xC0 | (cp >> 6) as u8;
            out[o + 1] = 0x80 | (cp & 0x3F) as u8;
        } else if n == 3 {
            out[o] = 0xE0 | (cp >> 12) as u8;
            out[o + 1] = 0x80 | ((cp >> 6) & 0x3F) as u8;
            out[o + 2] = 0x80 | (cp & 0x3F) as u8;
        } else {
            out[o] = 0xF0 | (cp >> 18) as u8;
            out[o + 1] = 0x80 | ((cp >> 12) & 0x3F) as u8;
            out[o + 2] = 0x80 | ((cp >> 6) & 0x3F) as u8;
            out[o + 3] = 0x80 | (cp & 0x3F) as u8;
        }
        o += n;
    }
    Some(o)
}
