//! POSIX-round-1 syscall entry points (P1): user-pointer copies plus the
//! tmpfs/fd semantics. `kernel_core::syscall::dispatch` routes v2 numbers
//! here; the arch installs `user::UserMemOps` (x86_64 stac/clac bridge). A
//! missing bridge yields ERR_NOSYS, so kernels without user mode are
//! unaffected.
//!
//! P1 limits (documented in docs/POSIX_PLAN.md): no fork/exec, no signals,
//! mmap is a stub, getdents returns one Fantuan-native 72-byte record per
//! call, console reads return EOF, and paths are capped at 255 bytes.

use fantuan_abi::{
    SYS_ERR_FAULT, SYS_ERR_INVAL, SYS_ERR_NOSYS, SYS_ERR_NOTTY, SYS_OK, Stat, Termios, Timespec,
    Winsize,
};

use crate::task;
use crate::time;
use crate::user;

use super::dir;
use super::fd;
use super::io;
use super::tmpfs;

const PATH_MAX: usize = 256;
const CHUNK: usize = 256;

fn nodeps() -> u64 {
    SYS_ERR_NOSYS
}

/// Copy a NUL-terminated user string into a kernel buffer (no NUL stored).
pub(crate) fn cpath(ptr: u64) -> Result<([u8; PATH_MAX], usize), u64> {
    let mut buf = [0u8; PATH_MAX];
    for i in 0..PATH_MAX {
        let mut b = [0u8; 1];
        user::copy_in(&mut b, ptr + i as u64).ok_or(SYS_ERR_FAULT)?;
        if b[0] == 0 {
            return Ok((buf, i));
        }
        buf[i] = b[0];
    }
    Err(fantuan_abi::SYS_ERR_NAMETOOLONG)
}

pub(crate) fn cpath_slice<'a>(buf: &'a [u8; PATH_MAX], len: usize) -> &'a [u8] {
    &buf[..len]
}

pub fn open(path: u64, flags: u64, _mode: u64) -> u64 {
    let (buf, len) = match cpath(path) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let slot = task::current_slot();
    match fd::open(slot, cpath_slice(&buf, len), flags as u32) {
        Ok(fd) => fd as u64,
        Err(e) => e,
    }
}

pub fn close(fd: u64) -> u64 {
    let slot = task::current_slot();
    match fd::close(slot, fd as u16) {
        Ok(()) => SYS_OK,
        Err(e) => e,
    }
}

pub fn read(fd: u64, buf: u64, len: u64) -> u64 {
    let slot = task::current_slot();
    let mut kbuf = [0u8; CHUNK];
    let mut done = 0usize;
    while (done as u64) < len {
        let want = (len - done as u64).min(CHUNK as u64) as usize;
        let n = match io::read(slot, fd as u16, &mut kbuf[..want]) {
            Ok(n) => n,
            Err(e) => return if done > 0 { done as u64 } else { e },
        };
        if n == 0 {
            break;
        }
        if user::copy_out(buf + done as u64, &kbuf[..n]).is_none() {
            return SYS_ERR_FAULT;
        }
        done += n;
        if n < want {
            break; // short read: EOF or a pipe with just this much available
        }
    }
    done as u64
}

pub fn write(fd: u64, buf: u64, len: u64) -> u64 {
    let slot = task::current_slot();
    let mut kbuf = [0u8; CHUNK];
    let mut done = 0usize;
    while (done as u64) < len {
        let want = (len - done as u64).min(CHUNK as u64) as usize;
        if user::copy_in(&mut kbuf[..want], buf + done as u64).is_none() {
            return SYS_ERR_FAULT;
        }
        let n = match io::write(slot, fd as u16, &kbuf[..want]) {
            Ok(n) => n,
            Err(e) => return if done > 0 { done as u64 } else { e },
        };
        done += n;
        if n < want {
            break;
        }
    }
    done as u64
}

pub fn lseek(fd: u64, off: u64, whence: u64) -> u64 {
    let slot = task::current_slot();
    match fd::lseek(slot, fd as u16, off as i64, whence as u32) {
        Ok(n) => n,
        Err(e) => e,
    }
}

/// Copy a Stat payload to user memory (shared with `posix_path`).
pub(super) fn put_stat(st: &Stat, ptr: u64) -> u64 {
    let bytes = unsafe {
        core::slice::from_raw_parts(st as *const Stat as *const u8, core::mem::size_of::<Stat>())
    };
    match user::copy_out(ptr, bytes) {
        Some(()) => SYS_OK,
        None => SYS_ERR_FAULT,
    }
}

pub fn fstat(fd: u64, st: u64) -> u64 {
    let slot = task::current_slot();
    match dir::fstat(slot, fd as u16) {
        Ok(s) => put_stat(&s, st),
        Err(e) => e,
    }
}

pub fn getdents(fd: u64, buf: u64, len: u64) -> u64 {
    if len < 72 {
        return SYS_ERR_INVAL;
    }
    let slot = task::current_slot();
    match dir::readdir(slot, fd as u16) {
        Ok(None) => 0,
        Ok(Some(de)) => {
            let bytes = unsafe {
                core::slice::from_raw_parts(
                    &de as *const fantuan_abi::Dirent as *const u8,
                    core::mem::size_of::<fantuan_abi::Dirent>(),
                )
            };
            match user::copy_out(buf, bytes) {
                Some(()) => 1,
                None => SYS_ERR_FAULT,
            }
        }
        Err(e) => e,
    }
}

pub fn pipe(fds: u64) -> u64 {
    let slot = task::current_slot();
    match fd::pipe(slot) {
        Ok((r, w)) => {
            let mut out = [0u8; 8];
            out[..4].copy_from_slice(&(r as u32).to_le_bytes());
            out[4..].copy_from_slice(&(w as u32).to_le_bytes());
            match user::copy_out(fds, &out) {
                Some(()) => SYS_OK,
                None => SYS_ERR_FAULT,
            }
        }
        Err(e) => e,
    }
}

pub fn dup(fd: u64) -> u64 {
    let slot = task::current_slot();
    match fd::dup(slot, fd as u16) {
        Ok(n) => n as u64,
        Err(e) => e,
    }
}

pub fn dup2(old: u64, new: u64) -> u64 {
    let slot = task::current_slot();
    match fd::dup2(slot, old as u16, new as u16) {
        Ok(n) => n as u64,
        Err(e) => e,
    }
}

pub fn ioctl(fd: u64, req: u64, arg: u64) -> u64 {
    let slot = task::current_slot();
    let Some(node) = fd::node_of(slot, fd as u16) else {
        return SYS_ERR_NOTTY;
    };
    if node != tmpfs::CONSOLE {
        return SYS_ERR_NOTTY;
    }
    match req {
        fantuan_abi::IOCTL_TCGETS => {
            if arg == 0 {
                return SYS_ERR_NOTTY;
            }
            let t = crate::tty::termios_get();
            put_bytes(arg, &t)
        }
        fantuan_abi::IOCTL_TCSETS => {
            if arg == 0 {
                return SYS_ERR_NOTTY;
            }
            let mut t = Termios::default();
            if copy_bytes(&mut t, arg).is_none() {
                return SYS_ERR_FAULT;
            }
            crate::tty::termios_set(&t)
        }
        fantuan_abi::IOCTL_TIOCGPGRP => {
            let p = crate::process::tty_pgrp();
            if user::copy_out(arg, &(p as u32).to_le_bytes()).is_none() {
                return SYS_ERR_FAULT;
            }
            SYS_OK
        }
        fantuan_abi::IOCTL_TIOCSPGRP => {
            let mut raw = [0u8; 4];
            if user::copy_in(&mut raw, arg).is_none() {
                return SYS_ERR_FAULT;
            }
            crate::process::set_tty_pgrp(u32::from_le_bytes(raw) as u64);
            SYS_OK
        }
        fantuan_abi::IOCTL_TIOCGWINSZ => {
            let w = Winsize { ws_row: 25, ws_col: 80, ws_xpixel: 0, ws_ypixel: 0 };
            put_bytes(arg, &w)
        }
        _ => fantuan_abi::SYS_ERR_NOTTY,
    }
}

fn put_bytes<T>(ptr: u64, v: &T) -> u64 {
    let bytes =
        unsafe { core::slice::from_raw_parts(v as *const T as *const u8, core::mem::size_of::<T>()) };
    match user::copy_out(ptr, bytes) {
        Some(()) => SYS_OK,
        None => SYS_ERR_FAULT,
    }
}

fn copy_bytes<T>(v: &mut T, ptr: u64) -> Option<()> {
    let bytes = unsafe {
        core::slice::from_raw_parts_mut(v as *mut T as *mut u8, core::mem::size_of::<T>())
    };
    user::copy_in(bytes, ptr)
}

/// wait4(pid, status, options, rusage): block on the process layer, encode
/// the status as the POSIX 32-bit wait word, ignore rusage (zero-filled).
pub fn wait4(pid: u64, status: u64, options: u64, rusage: u64) -> u64 {
    let _ = rusage;
    match crate::process::wait4(pid as i64, options) {
        Ok((child, st)) => {
            if status != 0 {
                if user::copy_out(status, &(st as u32).to_le_bytes()).is_none() {
                    return SYS_ERR_FAULT;
                }
            }
            child as u64
        }
        Err(e) => e,
    }
}

/// tcgetpgrp: the console's foreground group.
pub fn tcgetpgrp(fd: u64) -> u64 {
    let slot = task::current_slot();
    match fd::node_of(slot, fd as u16) {
        Some(n) if n == tmpfs::CONSOLE => crate::process::tty_pgrp(),
        _ => SYS_ERR_NOTTY,
    }
}

/// tcsetpgrp: only the console has a foreground group.
pub fn tcsetpgrp(fd: u64, pgrp: u64) -> u64 {
    let slot = task::current_slot();
    match fd::node_of(slot, fd as u16) {
        Some(n) if n == tmpfs::CONSOLE => {
            crate::process::set_tty_pgrp(pgrp);
            SYS_OK
        }
        _ => SYS_ERR_NOTTY,
    }
}

/// fcntl(fd, cmd, arg): F_DUPFD/F_GETFD/F_SETFD/F_GETFL/F_SETFL.
pub fn fcntl(fd: u64, cmd: u64, arg: u64) -> u64 {
    let slot = task::current_slot();
    match fd::fcntl(slot, fd as u16, cmd, arg) {
        Ok(v) => v,
        Err(e) => e,
    }
}

pub fn clock_gettime(clockid: u64, ts: u64) -> u64 {
    if clockid > 1 {
        return SYS_ERR_INVAL;
    }
    let ns = time::now_ns();
    let t = Timespec { tv_sec: (ns / 1_000_000_000) as i64, tv_nsec: (ns % 1_000_000_000) as i64 };
    let bytes = unsafe {
        core::slice::from_raw_parts(&t as *const Timespec as *const u8, core::mem::size_of::<Timespec>())
    };
    match user::copy_out(ts, bytes) {
        Some(()) => SYS_OK,
        None => SYS_ERR_FAULT,
    }
}

/// mmap is a P1 stub: brk-backed malloc covers the hello pipeline; the real
/// page mapping lands in P2 with the VMA list.
pub fn mmap() -> u64 {
    nodeps()
}
