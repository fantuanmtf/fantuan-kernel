//! The mounted read-only NTFS volume (M12-5), shared with the POSIX fd
//! layer. `vfs::init` sets it once, before any task runs, and it is never
//! mutated afterwards, so readers may share it without a lock.

use core::fmt::Write;

use crate::log::Log;
use crate::vfs::part;

use super::{Ntfs, NtfsErr, MAX_INDEX_BLOCK};

static mut MOUNT: Option<Ntfs> = None;

/// Mount the first readable NTFS volume: describe it, publish it for the
/// POSIX layer and return it with its partition index. A partition that
/// carries the NTFS OEM id but fails the boot/MFT parse is rejected with
/// its reason and stays probe-only.
pub fn mount_first(table: &part::Table, s: &mut Log) -> Option<(Ntfs, usize)> {
    for (pi, p) in table.parts[..table.count].iter().enumerate() {
        if !super::looks_like(p.first_lba) {
            continue;
        }
        match Ntfs::parse(p.first_lba) {
            Ok(n) => {
                n.describe(s);
                let _ = writeln!(s, "ntfs: mounted ro at /mnt/win0 (part {})", pi + 1);
                set_mount(n);
                return Some((n, pi));
            }
            Err(e) => {
                let _ = writeln!(s, "ntfs: part {} rejected: {}", pi + 1, e.text());
            }
        }
    }
    None
}

pub fn set_mount(n: Ntfs) {
    unsafe { *core::ptr::addr_of_mut!(MOUNT) = Some(n) };
}

pub fn mounted() -> bool {
    unsafe { (*core::ptr::addr_of!(MOUNT)).is_some() }
}

fn mount_ref() -> Option<&'static Ntfs> {
    unsafe { (*core::ptr::addr_of!(MOUNT)).as_ref() }
}

/// `/mnt/win0/...` (or `mnt/win0/...`) to a volume-relative path; Some("")
/// is the mount root.
pub fn win_rel(path: &[u8]) -> Option<&[u8]> {
    let p = path.strip_prefix(b"/").unwrap_or(path);
    let rest = p.strip_prefix(b"mnt/win0")?;
    if rest.is_empty() {
        Some(b"")
    } else {
        rest.strip_prefix(b"/")
    }
}

/// Resolve a `/mnt/win0` path: (record number, is_dir, size, attributes).
pub fn stat_path(path: &[u8]) -> Result<(u64, bool, u64, u32), NtfsErr> {
    let n = mount_ref().ok_or(NtfsErr::NotFound)?;
    let rel = win_rel(path).ok_or(NtfsErr::NotFound)?;
    let mut scratch = [0u8; MAX_INDEX_BLOCK];
    let rec = n.resolve(rel, &mut scratch)?;
    if !rec.is_used() {
        return Err(NtfsErr::NotFound);
    }
    Ok((rec.number, rec.is_dir(), rec.size(), rec.file_attributes()))
}

/// (size, is_dir, attributes) of a record, for fstat/lseek.
pub fn stat_record(recno: u64) -> Result<(u64, bool, u32), NtfsErr> {
    let n = mount_ref().ok_or(NtfsErr::NotFound)?;
    let rec = n.read_record(recno)?;
    Ok((rec.size(), rec.is_dir(), rec.file_attributes()))
}

/// Read at OFFSET from an open record (the POSIX open carries the number).
pub fn read_record_at(recno: u64, offset: u64, out: &mut [u8]) -> Result<usize, NtfsErr> {
    let n = mount_ref().ok_or(NtfsErr::NotFound)?;
    let rec = n.read_record(recno)?;
    n.read_at(&rec, offset, out)
}

/// The INDEX-th visible directory entry of RECNO, name copied into NAME.
pub fn dir_entry(
    recno: u64,
    index: u64,
    name: &mut [u8],
) -> Result<Option<(u64, bool, u64, usize)>, NtfsErr> {
    let n = mount_ref().ok_or(NtfsErr::NotFound)?;
    let rec = n.read_record(recno)?;
    if !rec.is_dir() {
        return Err(NtfsErr::NotDir);
    }
    let mut scratch = [0u8; MAX_INDEX_BLOCK];
    n.entry_at(&rec, index, &mut scratch, name)
}
