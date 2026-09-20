//! fd-level read/write for the P1 POSIX layer (tmpfs, console, pipes).
//!
//! Split from `fd.rs` to keep files within the repo's 300-line convention;
//! the open-file table and its lock live there. Console writes go through
//! `crate::log::put` (the kernel sink, the same channel as SYS_WRITE) and
//! console reads return EOF until the P2 line discipline exists.

use fantuan_abi::{SYS_ERR_BADF, SYS_ERR_ISDIR, O_APPEND};

use super::fd::{self, FS_LOCK};
use super::pipe;
use super::tmpfs::{self, Kind};
use crate::arch::IrqLock;

/// read(fd, out): files (honoring the open offset), console/EOF, null, pipes.
pub fn read(slot: usize, fd: u16, out: &mut [u8]) -> Result<usize, u64> {
    let (pipe_id, idx) = {
        let _g = IrqLock::acquire(&FS_LOCK);
        let Some(idx) = fd::open_of(slot, fd) else { return Err(SYS_ERR_BADF) };
        (fd::open_at(idx).pipe, idx)
    };
    if pipe_id != 0 {
        return pipe::read(pipe_id - 1, out);
    }
    let _g = IrqLock::acquire(&FS_LOCK);
    let o = fd::open_at(idx);
    let n = tmpfs::read(o.node, o.offset, out)?;
    fd::set_offset(idx, o.offset + n as u64);
    Ok(n)
}

/// write(fd, data): console sink, files with offsets and pipes.
pub fn write(slot: usize, fd: u16, data: &[u8]) -> Result<usize, u64> {
    let (idx, pipe_id) = {
        let _g = IrqLock::acquire(&FS_LOCK);
        let idx = fd::open_of(slot, fd).ok_or(SYS_ERR_BADF)?;
        (idx, fd::open_at(idx).pipe)
    };
    if pipe_id != 0 {
        return pipe::write(pipe_id - 1, data);
    }
    let _g = IrqLock::acquire(&FS_LOCK);
    let o = fd::open_at(idx);
    match tmpfs::kind(o.node) {
        Kind::Console => {
            crate::log::put(data);
            Ok(data.len())
        }
        Kind::Null => Ok(data.len()),
        Kind::Dir => Err(SYS_ERR_ISDIR),
        Kind::File => {
            let off = if o.flags & O_APPEND as u32 != 0 { tmpfs::size(o.node) } else { o.offset };
            let n = tmpfs::write(o.node, off, data)?;
            fd::set_offset(idx, off + n as u64);
            Ok(n)
        }
    }
}
