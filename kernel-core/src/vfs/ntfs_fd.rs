//! POSIX fd glue for the read-only /mnt/win0 NTFS mount (M12-5). The open
//! path refuses every write intent with EROFS before any filesystem code
//! runs; reads, readdir and stat are served from the mounted volume. Kept
//! out of fd.rs/io.rs/dir.rs so those files stay small.

use fantuan_abi::{
    Stat, O_APPEND, O_CREAT, O_RDWR, O_TRUNC, O_WRONLY, SYS_ERR_IO, SYS_ERR_MFILE, SYS_ERR_NOENT,
    SYS_ERR_NOTDIR, SYS_ERR_ROFS,
};

use super::fd;
use super::ntfs::{self, NtfsErr};
use super::tmpfs;

/// True when PATH names anything under the /mnt/win0 mount.
pub fn is_win(path: &[u8]) -> bool {
    ntfs::win_rel(path).is_some()
}

/// NTFS read errors as Fantuan errno values.
pub fn errno(e: NtfsErr) -> u64 {
    match e {
        NtfsErr::NotFound => SYS_ERR_NOENT,
        NtfsErr::NotDir => SYS_ERR_NOTDIR,
        _ => SYS_ERR_IO,
    }
}

/// open(2) on /mnt/win0: read-only opens carry the MFT record number.
pub fn open(slot: usize, path: &[u8], flags: u64) -> Result<u16, u64> {
    if flags & (O_WRONLY | O_RDWR | O_TRUNC | O_APPEND | O_CREAT) != 0 {
        return Err(SYS_ERR_ROFS);
    }
    let (rec, _is_dir, _size, _attrs) = ntfs::stat_path(path).map_err(errno)?;
    let Some(open) = fd::alloc_open() else { return Err(SYS_ERR_MFILE) };
    let Some(fd) = fd::install(slot, open, flags as u32, tmpfs::ROOT, 0) else {
        fd::opens()[open] = fd::EMPTY_OPEN;
        return Err(SYS_ERR_MFILE);
    };
    fd::opens()[open].ntfs = rec + 1;
    Ok(fd)
}

/// read(2) from an open NTFS record.
pub fn read(recno: u64, offset: u64, out: &mut [u8]) -> Result<usize, u64> {
    ntfs::read_record_at(recno, offset, out).map_err(errno)
}

/// fstat(2) of an open NTFS record.
pub fn stat(recno: u64) -> Result<Stat, u64> {
    let (size, is_dir, attrs) = ntfs::stat_record(recno).map_err(errno)?;
    Ok(synth_stat(recno, size, is_dir, attrs))
}

/// stat(2) by /mnt/win0 path.
pub fn path_stat(path: &[u8]) -> Result<Stat, u64> {
    let (rec, is_dir, size, attrs) = ntfs::stat_path(path).map_err(errno)?;
    Ok(synth_stat(rec, size, is_dir, attrs))
}

/// The INDEX-th visible entry of an open directory: (child, is_dir, name).
pub fn dir_entry(recno: u64, index: u64) -> Result<Option<(u64, bool, [u8; 128], usize)>, u64> {
    let mut name = [0u8; 128];
    match ntfs::dir_entry(recno, index, &mut name).map_err(errno)? {
        None => Ok(None),
        Some((child, is_dir, _size, len)) => Ok(Some((child, is_dir, name, len))),
    }
}

/// Synthetic stat for an NTFS object on the read-only mount. The file mode
/// carries the on-disk read-only flag (bit 0x01).
fn synth_stat(rec: u64, size: u64, is_dir: bool, attrs: u32) -> Stat {
    use fantuan_abi::{S_IFDIR, S_IFREG};
    let mut st = tmpfs::stat(tmpfs::ROOT);
    st.st_ino = rec + 1;
    let mode = if is_dir { 0o555 } else if attrs & 0x01 != 0 { 0o444 } else { 0o644 };
    st.st_mode = if is_dir { S_IFDIR | mode } else { S_IFREG | mode };
    st.st_size = size;
    st.st_blocks = (size + 511) / 512;
    st.st_nlink = 1;
    st
}
