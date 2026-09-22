//! fd-level read/write for the POSIX layer (tmpfs, console, pipes).
//!
//! Split from `fd.rs` to keep files within the repo's 300-line convention;
//! the open-file table and its lock live there. Console writes go through
//! `crate::log::put` (the kernel sink, the same channel as SYS_WRITE) and
//! console reads go through the P2 `tty` line discipline, which blocks by
//! sleeping outside the FS lock.

use fantuan_abi::{SYS_ERR_ACCES, SYS_ERR_BADF, SYS_ERR_ISDIR, O_APPEND};

use super::fd::{self, FS_LOCK};
use super::pipe;
use super::tmpfs::{self, Kind};
use crate::arch::IrqLock;

/// Copy out of a read-only registry file at `off` (bounded by its length).
fn rom_read(ptr: u64, len: u64, off: u64, out: &mut [u8]) -> usize {
    if off >= len {
        return 0;
    }
    let take = (len - off).min(out.len() as u64) as usize;
    let src = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    out[..take].copy_from_slice(&src[off as usize..off as usize + take]);
    take
}

/// read(fd, out): files (honoring the open offset), console/tty, null, pipes.
pub fn read(slot: usize, fd: u16, out: &mut [u8]) -> Result<usize, u64> {
    let (pipe_id, idx, node, rom, rom_len, offset) = {
        let _g = IrqLock::acquire(&FS_LOCK);
        let Some(idx) = fd::open_of(slot, fd) else { return Err(SYS_ERR_BADF) };
        let o = fd::open_at(idx);
        (o.pipe, idx, o.node, o.rom, o.rom_len, o.offset)
    };
    if pipe_id != 0 {
        return pipe::read(pipe_id - 1, out);
    }
    #[cfg(kconfig_ntfs)]
    {
        let ntfs = {
            let _g = IrqLock::acquire(&FS_LOCK);
            fd::open_at(idx).ntfs
        };
        if ntfs != 0 {
            let n = super::ntfs_fd::read(ntfs - 1, offset, out)?;
            fd::set_offset(idx, offset + n as u64);
            return Ok(n);
        }
    }
    if rom != 0 {
        let n = rom_read(rom, rom_len, offset, out);
        fd::set_offset(idx, offset + n as u64);
        return Ok(n);
    }
    if tmpfs::kind(node) == Kind::Console {
        return crate::tty::read(out);
    }
    let _g = IrqLock::acquire(&FS_LOCK);
    let o = fd::open_at(idx);
    let n = tmpfs::read(o.node, o.offset, out)?;
    fd::set_offset(idx, o.offset + n as u64);
    Ok(n)
}

/// Kernel-side create-or-truncate of a tmpfs file, no fd involved (the
/// imager report writer). Takes the shared FS lock like the fd path.
pub fn write_mem_file(dir: &[u8], name: &[u8], data: &[u8]) -> bool {
    let _g = IrqLock::acquire(&FS_LOCK);
    let Ok(d) = tmpfs::lookup(tmpfs::ROOT, dir) else { return false };
    let id = match tmpfs::lookup(d, name) {
        Ok(id) => {
            tmpfs::truncate(id);
            id
        }
        Err(_) => match tmpfs::create(d, name, Kind::File) {
            Ok(id) => id,
            Err(_) => return false,
        },
    };
    tmpfs::write(id, 0, data).is_ok()
}

/// write(fd, data): console sink, files with offsets and pipes.
pub fn write(slot: usize, fd: u16, data: &[u8]) -> Result<usize, u64> {
    let (idx, pipe_id, rom) = {
        let _g = IrqLock::acquire(&FS_LOCK);
        let idx = fd::open_of(slot, fd).ok_or(SYS_ERR_BADF)?;
        let o = fd::open_at(idx);
        (idx, o.pipe, o.rom)
    };
    if pipe_id != 0 {
        return pipe::write(pipe_id - 1, data);
    }
    if rom != 0 {
        return Err(SYS_ERR_ACCES); // embedded binaries are read-only
    }
    let _g = IrqLock::acquire(&FS_LOCK);
    let o = fd::open_at(idx);
    #[cfg(kconfig_ntfs)]
    if o.ntfs != 0 {
        return Err(fantuan_abi::SYS_ERR_ROFS); // /mnt/win0 has no write path
    }
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
