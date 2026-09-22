//! Path-based POSIX syscalls (P1): stat/chdir/getcwd/unlink/rmdir/mkdir/
//! rename. Split from `posix.rs` to keep files within the repo's 300-line
//! convention; `posix.rs` owns open/close/read/write and the user copies.

use fantuan_abi::{
    SYS_ERR_FAULT, SYS_ERR_INVAL, SYS_ERR_ISDIR, SYS_OK,
};
#[cfg(kconfig_ntfs)]
use fantuan_abi::SYS_ERR_ROFS;

use crate::task;
use crate::user;

use super::dir;
use super::fd;
use super::posix::{cpath, cpath_slice, put_stat};
use super::tmpfs::{self, Kind};

/// A write against the read-only /mnt/win0 mount is refused with EROFS.
#[cfg(kconfig_ntfs)]
fn win_ro(path: &[u8]) -> bool {
    super::ntfs_fd::is_win(path)
}

pub fn stat(path: u64, st: u64) -> u64 {
    let (buf, len) = match cpath(path) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let slot = task::current_slot();
    let cwd = fd::cwd(slot);
    let path = cpath_slice(&buf, len);
    #[cfg(kconfig_ntfs)]
    if win_ro(path) {
        return match super::ntfs_fd::path_stat(path) {
            Ok(s) => put_stat(&s, st),
            Err(e) => e,
        };
    }
    match tmpfs::lookup(cwd, path) {
        Ok(node) => put_stat(&tmpfs::stat(node), st),
        // Embedded-binary registry: /bin entries without a tmpfs node still
        // stat as regular 0755 files (dash's PATH search needs this).
        Err(e) => match fd::rom_stat(path) {
            Some(rom) => put_stat(&rom, st),
            None => e,
        },
    }
}

pub fn chdir(path: u64) -> u64 {
    let (buf, len) = match cpath(path) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let slot = task::current_slot();
    match dir::chdir(slot, cpath_slice(&buf, len)) {
        Ok(()) => SYS_OK,
        Err(e) => e,
    }
}

pub fn getcwd(buf: u64, len: u64) -> u64 {
    if len == 0 || len > 4096 {
        return SYS_ERR_INVAL;
    }
    let slot = task::current_slot();
    let mut kbuf = [0u8; 4096];
    let n = match dir::getcwd(slot, &mut kbuf[..len as usize]) {
        Ok(n) => n,
        Err(e) => return e,
    };
    match user::copy_out(buf, &kbuf[..n]) {
        Some(()) => n as u64,
        None => SYS_ERR_FAULT,
    }
}

pub fn unlink(path: u64) -> u64 {
    path_remove(path, false)
}

pub fn rmdir(path: u64) -> u64 {
    path_remove(path, true)
}

fn path_remove(path: u64, dir: bool) -> u64 {
    let (buf, len) = match cpath(path) {
        Ok(v) => v,
        Err(e) => return e,
    };
    #[cfg(kconfig_ntfs)]
    if win_ro(cpath_slice(&buf, len)) {
        return SYS_ERR_ROFS;
    }
    let slot = task::current_slot();
    let cwd = fd::cwd(slot);
    match tmpfs::lookup(cwd, cpath_slice(&buf, len)) {
        Ok(node) => {
            let is_dir = tmpfs::kind(node) == Kind::Dir;
            if is_dir != dir {
                return SYS_ERR_ISDIR;
            }
            match tmpfs::remove(node) {
                Ok(()) => SYS_OK,
                Err(e) => e,
            }
        }
        Err(e) => e,
    }
}

pub fn mkdir(path: u64, _mode: u64) -> u64 {
    let (buf, len) = match cpath(path) {
        Ok(v) => v,
        Err(e) => return e,
    };
    #[cfg(kconfig_ntfs)]
    if win_ro(cpath_slice(&buf, len)) {
        return SYS_ERR_ROFS;
    }
    let slot = task::current_slot();
    match fd::create_in(slot, cpath_slice(&buf, len), Kind::Dir) {
        Ok(_) => SYS_OK,
        Err(e) => e,
    }
}

pub fn rename(old: u64, new: u64) -> u64 {
    let (ob, ol) = match cpath(old) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let (nb, nl) = match cpath(new) {
        Ok(v) => v,
        Err(e) => return e,
    };
    #[cfg(kconfig_ntfs)]
    if win_ro(cpath_slice(&ob, ol)) || win_ro(cpath_slice(&nb, nl)) {
        return SYS_ERR_ROFS;
    }
    let slot = task::current_slot();
    match fd::rename_in(slot, cpath_slice(&ob, ol), cpath_slice(&nb, nl)) {
        Ok(()) => SYS_OK,
        Err(e) => e,
    }
}
