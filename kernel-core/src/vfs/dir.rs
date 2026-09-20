//! Directory, stat and cwd path for the P1 POSIX layer.
//!
//! Split from `fd.rs` to keep files within the repo's 300-line convention;
//! `getdents` returns one Fantuan-native 72-byte record per call and the
//! open offset is the directory cursor.

use fantuan_abi::{
    Dirent, Stat, DT_CHR, DT_DIR, DT_REG, SYS_ERR_BADF, SYS_ERR_FAULT, SYS_ERR_NOTDIR, S_IFIFO,
};

use super::fd::{self, FS_LOCK};
use super::pipe;
use super::tmpfs::{self, Kind};
use crate::arch::IrqLock;

/// One dirent per call from the fd's directory offset; None at end.
pub fn readdir(slot: usize, fd: u16) -> Result<Option<Dirent>, u64> {
    let _g = IrqLock::acquire(&FS_LOCK);
    let Some(idx) = fd::open_of(slot, fd) else { return Err(SYS_ERR_BADF) };
    let o = fd::open_at(idx);
    if o.pipe != 0 {
        return Err(SYS_ERR_NOTDIR);
    }
    if tmpfs::kind(o.node) != Kind::Dir {
        return Err(SYS_ERR_NOTDIR);
    }
    let index = o.offset as usize;
    let Some((child, _name_len)) = tmpfs::dir_entry(o.node, index) else {
        return Ok(None);
    };
    let mut de = Dirent {
        d_ino: child as u64 + 1,
        d_type: dirent_type(child),
        d_reclen: 72,
        d_name: [0; 56],
    };
    let mut name = [0u8; 56];
    let n = tmpfs::copy_name(child, &mut name);
    de.d_name[..n.min(56)].copy_from_slice(&name[..n.min(56)]);
    fd::set_offset(idx, index as u64 + 1);
    Ok(Some(de))
}

fn dirent_type(child: u16) -> u32 {
    match tmpfs::kind(child) {
        Kind::Dir => DT_DIR,
        Kind::File => DT_REG,
        _ => DT_CHR,
    }
}

/// fstat: tmpfs nodes plus pipes (S_IFIFO, size = unread bytes).
pub fn fstat(slot: usize, fd: u16) -> Result<Stat, u64> {
    let _g = IrqLock::acquire(&FS_LOCK);
    let Some(idx) = fd::open_of(slot, fd) else { return Err(SYS_ERR_BADF) };
    let o = fd::open_at(idx);
    if o.pipe != 0 {
        let pid = o.pipe - 1;
        let mut st = tmpfs::stat(tmpfs::ROOT);
        st.st_ino = 1000 + pid as u64;
        st.st_mode = S_IFIFO | 0o600;
        st.st_size = pipe::unread(pid);
        return Ok(st);
    }
    Ok(tmpfs::stat(o.node))
}

pub fn chdir(slot: usize, path: &[u8]) -> Result<(), u64> {
    let _g = IrqLock::acquire(&FS_LOCK);
    let node = tmpfs::lookup(fd::cwd_get(slot), path)?;
    if tmpfs::kind(node) != Kind::Dir {
        return Err(SYS_ERR_NOTDIR);
    }
    fd::cwd_set(slot, node);
    Ok(())
}

/// Absolute path of the cwd into `out`; returns the length.
pub fn getcwd(slot: usize, out: &mut [u8]) -> Result<usize, u64> {
    let _g = IrqLock::acquire(&FS_LOCK);
    let mut names = [0u16; 8];
    let mut n = 0;
    let mut cur = fd::cwd_get(slot);
    while cur != tmpfs::ROOT && n < names.len() {
        names[n] = cur;
        n += 1;
        cur = tmpfs::parent(cur);
    }
    let mut len = 0;
    if n == 0 {
        if out.is_empty() {
            return Err(SYS_ERR_FAULT);
        }
        out[0] = b'/';
        return Ok(1);
    }
    for i in (0..n).rev() {
        if len >= out.len() {
            return Err(SYS_ERR_FAULT);
        }
        out[len] = b'/';
        len += 1;
        let mut name = [0u8; 32];
        let nl = tmpfs::copy_name(names[i], &mut name);
        for &b in &name[..nl] {
            if len >= out.len() {
                return Err(SYS_ERR_FAULT);
            }
            out[len] = b;
            len += 1;
        }
    }
    Ok(len)
}
