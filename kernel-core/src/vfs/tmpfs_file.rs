//! tmpfs regular-file I/O, stat and directory enumeration (P1).
//!
//! Split from `tmpfs.rs` to keep files within the repo's 300-line convention;
//! the node table and tree operations live there. The 64 KiB pool is a static
//! byte array bump-allocated in FILE_CAP chunks by `reserve` (no reclaim on
//! delete in P1), so a regular file never exceeds 8 KiB.

use fantuan_abi::{Stat, S_IFCHR, S_IFDIR, S_IFREG, SYS_ERR_ISDIR, SYS_ERR_NOSPC};

use super::tmpfs::{self, Kind};

/// P1 layouts are mirrored by libc headers; this pins the shared size.
const _: () = assert!(core::mem::size_of::<Stat>() == 80);

const POOL_BYTES: usize = tmpfs::MAX_FILES * tmpfs::FILE_CAP;

static mut POOL: [u8; POOL_BYTES] = [0; POOL_BYTES];
static mut POOL_END: usize = 0;

/// Reserve one FILE_CAP chunk for a new regular file (bump allocator);
/// returns the offset into the pool. Caller holds the FS lock via create().
pub(super) fn reserve() -> Result<u32, u64> {
    let off = unsafe { POOL_END };
    if off + tmpfs::FILE_CAP > POOL_BYTES {
        return Err(SYS_ERR_NOSPC);
    }
    unsafe { POOL_END = off + tmpfs::FILE_CAP };
    Ok(off as u32)
}

/// Read up to `buf.len()` bytes at `off` from a regular file.
pub fn read(id: u16, off: u64, buf: &mut [u8]) -> Result<usize, u64> {
    let n = tmpfs::node(id);
    if n.kind != Kind::File {
        return match n.kind {
            Kind::Dir => Err(SYS_ERR_ISDIR),
            _ => Ok(0), // console reads EOF, null is always empty
        };
    }
    if off >= n.size as u64 {
        return Ok(0);
    }
    let start = (n.off as usize) + off as usize;
    let take = (n.size as u64 - off).min(buf.len() as u64) as usize;
    buf[..take].copy_from_slice(unsafe { &POOL[start..start + take] });
    Ok(take)
}

/// Write at `off` into a regular file; cannot extend past FILE_CAP.
pub fn write(id: u16, off: u64, data: &[u8]) -> Result<usize, u64> {
    let n = tmpfs::node_mut(id);
    if n.kind != Kind::File {
        return Err(SYS_ERR_ISDIR);
    }
    if off + data.len() as u64 > tmpfs::FILE_CAP as u64 {
        return Err(SYS_ERR_NOSPC);
    }
    let start = n.off as usize + off as usize;
    unsafe { POOL[start..start + data.len()].copy_from_slice(data) };
    n.size = n.size.max((off + data.len() as u64) as u32);
    Ok(data.len())
}

pub fn truncate(id: u16) {
    tmpfs::node_mut(id).size = 0;
}

pub fn size(id: u16) -> u64 {
    tmpfs::node(id).size as u64
}

/// P1 stat: node kind to mode, size in bytes, nanosecond times from boot.
pub fn stat(id: u16) -> Stat {
    let n = tmpfs::node(id);
    let mode = match n.kind {
        Kind::Dir => S_IFDIR | 0o755,
        Kind::File => S_IFREG | 0o644,
        Kind::Console | Kind::Null => S_IFCHR | 0o666,
    };
    let now = crate::time::now_ns();
    Stat {
        st_dev: 1,
        st_ino: id as u64 + 1,
        st_size: n.size as u64,
        st_blocks: (n.size as u64 + 511) / 512,
        st_blksize: 512,
        st_mode: mode as u32,
        st_nlink: if n.kind == Kind::Dir { 2 } else { 1 },
        st_uid: 0,
        st_gid: 0,
        st_atime_ns: now,
        st_mtime_ns: now,
        st_ctime_ns: now,
    }
}

/// Number of children of a directory (stable while the FS lock is held).
pub fn dir_count(dir: u16) -> usize {
    (0..tmpfs::MAX_NODES as u16)
        .filter(|&i| tmpfs::node(i).used && tmpfs::node(i).parent == dir && i != dir)
        .count()
}

/// (child, name length) for the `index`-th child of `dir`.
pub fn dir_entry(dir: u16, index: usize) -> Option<(u16, usize)> {
    (0..tmpfs::MAX_NODES as u16)
        .filter(|&i| tmpfs::node(i).used && tmpfs::node(i).parent == dir && i != dir)
        .nth(index)
        .map(|i| (i, tmpfs::node(i).name_len as usize))
}

/// Copy a node's name into `buf`; returns the length.
pub fn copy_name(id: u16, buf: &mut [u8]) -> usize {
    let n = tmpfs::node(id);
    let len = (n.name_len as usize).min(buf.len());
    buf[..len].copy_from_slice(&n.name[..len]);
    len
}

/// Node kind for a `d_type` dirent field.
pub fn dirent_type(id: u16) -> u32 {
    match tmpfs::node(id).kind {
        Kind::Dir => 4,                  // DT_DIR
        Kind::File => 8,                 // DT_REG
        Kind::Console | Kind::Null => 2, // DT_CHR
    }
}

const _: () = assert!(core::mem::size_of::<Stat>() == 80);