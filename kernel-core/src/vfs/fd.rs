//! Per-task file-descriptor table and open-file objects for P1.
//!
//! Fixed static tables (no kernel heap): 16 fds per task and 64 open
//! objects; `dup`/`dup2` share the open object (offset and file-description
//! semantics). The shared `FS_LOCK` serializes every table mutation;
//! `close_all` runs at task exit/reap so fds cannot leak across slots.
//! Pipe buffers live in `vfs::pipe`, the read/write path in `vfs::io`, and
//! the directory/stat path in `vfs::dir`.

use core::sync::atomic::AtomicBool;

use fantuan_abi::{
    O_APPEND, O_CREAT, O_EXCL, O_RDONLY, O_TRUNC, O_WRONLY, SYS_ERR_BADF, SYS_ERR_EXIST,
    SYS_ERR_INVAL, SYS_ERR_ISDIR, SYS_ERR_MFILE, SYS_ERR_NOENT, SYS_ERR_SPIPE,
};

use crate::arch::IrqLock;

use super::pipe;
use super::tmpfs::{self, Kind};

pub const MAX_FDS: usize = 16;
const MAX_OPEN: usize = 64;
const MAX_TASKS: usize = crate::task::MAX_TASKS;

/// Serializes the P1 FS tables (tmpfs, fds, pipes); `pub(super)` so the
/// sibling VFS modules share one lock.
pub(super) static FS_LOCK: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy)]
struct Fd {
    open: u16, // open index + 1; 0 = free
}

const EMPTY_FD: Fd = Fd { open: 0 };

#[derive(Clone, Copy)]
pub(super) struct Open {
    pub used: bool,
    pub node: u16,
    pub flags: u32,
    pub offset: u64,
    pub refs: u8,
    pub pipe: u16, // pipe index + 1; 0 = not a pipe
}

pub(super) const EMPTY_OPEN: Open =
    Open { used: false, node: 0, flags: 0, offset: 0, refs: 0, pipe: 0 };

static mut FDS: [[Fd; MAX_FDS]; MAX_TASKS] = [[EMPTY_FD; MAX_FDS]; MAX_TASKS];
static mut OPENS: [Open; MAX_OPEN] = [EMPTY_OPEN; MAX_OPEN];
static mut CWD: [u16; MAX_TASKS] = [tmpfs::ROOT; MAX_TASKS];

fn fds(slot: usize) -> &'static mut [Fd; MAX_FDS] {
    unsafe { &mut *core::ptr::addr_of_mut!(FDS[slot]) }
}

pub(super) fn opens() -> &'static mut [Open; MAX_OPEN] {
    unsafe { &mut *core::ptr::addr_of_mut!(OPENS) }
}

pub(super) fn cwd_get(slot: usize) -> u16 {
    unsafe { *core::ptr::addr_of!(CWD[slot]) }
}

pub(super) fn cwd_set(slot: usize, v: u16) {
    unsafe { *core::ptr::addr_of_mut!(CWD[slot]) = v };
}

pub(super) fn open_of(slot: usize, fd: u16) -> Option<usize> {
    let e = fds(slot).get(fd as usize)?;
    if e.open == 0 {
        None
    } else {
        Some(e.open as usize - 1)
    }
}

pub(super) fn open_at(idx: usize) -> Open {
    opens()[idx]
}

pub(super) fn set_offset(idx: usize, off: u64) {
    opens()[idx].offset = off;
}

fn alloc_open() -> Option<usize> {
    (0..MAX_OPEN).find(|&i| !opens()[i].used)
}

fn install(slot: usize, open: usize, flags: u32, node: u16, pipe_id: u16) -> Option<u16> {
    let fd = (0..MAX_FDS).find(|&i| fds(slot)[i].open == 0)?;
    opens()[open] = Open { used: true, node, flags, offset: 0, refs: 1, pipe: pipe_id };
    fds(slot)[fd] = Fd { open: open as u16 + 1 };
    Some(fd as u16)
}

/// Initialize a task's fd table: fds 0..2 on /dev/console, cwd "/".
pub fn init_task(slot: usize) {
    let _g = IrqLock::acquire(&FS_LOCK);
    close_all_unlocked(slot);
    cwd_set(slot, tmpfs::ROOT);
    let Some(open) = alloc_open() else { return };
    opens()[open] = Open {
        used: true,
        node: tmpfs::CONSOLE,
        flags: O_RDONLY as u32,
        offset: 0,
        refs: 3,
        pipe: 0,
    };
    for fd in 0..3 {
        fds(slot)[fd] = Fd { open: open as u16 + 1 };
    }
}

fn close_open(open_idx: usize) {
    let mut o = opens()[open_idx];
    if o.refs > 0 {
        o.refs -= 1;
        opens()[open_idx].refs = o.refs;
    }
    if o.refs == 0 {
        if o.pipe != 0 {
            pipe::drop_end(o.pipe - 1, o.flags & O_WRONLY as u32 != 0);
        }
        opens()[open_idx] = EMPTY_OPEN;
    }
}

fn close_all_unlocked(slot: usize) {
    for i in 0..MAX_FDS {
        let fd = fds(slot)[i];
        if fd.open != 0 {
            close_open(fd.open as usize - 1);
            fds(slot)[i] = EMPTY_FD;
        }
    }
}

/// Release every fd of a task (task exit / reap / table reuse).
pub fn close_all(slot: usize) {
    let _g = IrqLock::acquire(&FS_LOCK);
    close_all_unlocked(slot);
}

/// open(path, flags) at the fd layer (mode is ignored: root).
pub fn open(slot: usize, path: &[u8], flags: u32) -> Result<u16, u64> {
    let _g = IrqLock::acquire(&FS_LOCK);
    let flags64 = flags as u64;
    let node = match tmpfs::lookup(cwd_get(slot), path) {
        Ok(n) => {
            if flags64 & O_CREAT != 0 && flags64 & O_EXCL != 0 {
                return Err(SYS_ERR_EXIST);
            }
            n
        }
        Err(e) if e == SYS_ERR_NOENT && flags64 & O_CREAT != 0 => {
            let cwd = cwd_get(slot);
            let (dir, name) = split_parent(cwd, path)?;
            tmpfs::create(dir, name, Kind::File)?
        }
        Err(e) => return Err(e),
    };
    let kind = tmpfs::kind(node);
    if kind == Kind::Dir && flags64 & (O_WRONLY | O_APPEND) != 0 {
        return Err(SYS_ERR_ISDIR);
    }
    if flags64 & O_TRUNC != 0 {
        tmpfs::truncate(node);
    }
    let Some(open) = alloc_open() else { return Err(SYS_ERR_MFILE) };
    install(slot, open, flags, node, 0).ok_or(SYS_ERR_MFILE)
}

/// Split a path's last component for create/rename (missing files only).
fn split_parent(cwd: u16, path: &[u8]) -> Result<(u16, &[u8]), u64> {
    let Some(slash) = path.iter().rposition(|&b| b == b'/') else {
        return Ok((cwd, path));
    };
    let name = &path[slash + 1..];
    if name.is_empty() {
        return Err(SYS_ERR_NOENT);
    }
    let dir_path = if slash == 0 { &path[..1] } else { &path[..slash] };
    Ok((tmpfs::lookup(cwd, dir_path)?, name))
}

/// cwd node of a task (tmpfs lookups from the path syscalls).
pub fn cwd(slot: usize) -> u16 {
    cwd_get(slot)
}

/// mkdir path helper: create the last component under its parent.
pub fn create_in(slot: usize, path: &[u8], kind: Kind) -> Result<u16, u64> {
    let _g = IrqLock::acquire(&FS_LOCK);
    let (dir, name) = split_parent(cwd_get(slot), path)?;
    tmpfs::create(dir, name, kind)
}

/// rename path helper: resolve the source, split the destination.
pub fn rename_in(slot: usize, old: &[u8], new: &[u8]) -> Result<(), u64> {
    let _g = IrqLock::acquire(&FS_LOCK);
    let src = tmpfs::lookup(cwd_get(slot), old)?;
    let (dir, name) = split_parent(cwd_get(slot), new)?;
    tmpfs::rename(src, dir, name)
}

pub fn close(slot: usize, fd: u16) -> Result<(), u64> {
    let _g = IrqLock::acquire(&FS_LOCK);
    let Some(open) = open_of(slot, fd) else { return Err(SYS_ERR_BADF) };
    close_open(open);
    fds(slot)[fd as usize] = EMPTY_FD;
    Ok(())
}

pub fn lseek(slot: usize, fd: u16, off: i64, whence: u32) -> Result<u64, u64> {
    let _g = IrqLock::acquire(&FS_LOCK);
    let Some(idx) = open_of(slot, fd) else { return Err(SYS_ERR_BADF) };
    let o = opens()[idx];
    if o.pipe != 0 {
        return Err(SYS_ERR_SPIPE);
    }
    let base = match whence {
        0 => 0,
        1 => o.offset as i64,
        2 => tmpfs::size(o.node) as i64,
        _ => return Err(SYS_ERR_INVAL),
    };
    let next = base.checked_add(off).ok_or(SYS_ERR_INVAL)?;
    if next < 0 {
        return Err(SYS_ERR_INVAL);
    }
    opens()[idx].offset = next as u64;
    Ok(next as u64)
}

fn dup_into(slot: usize, fd: u16) -> Result<u16, u64> {
    let old = open_of(slot, fd).ok_or(SYS_ERR_BADF)?;
    let Some(new_fd) = (0..MAX_FDS).find(|&i| fds(slot)[i].open == 0) else {
        return Err(SYS_ERR_MFILE);
    };
    opens()[old].refs += 1;
    fds(slot)[new_fd] = Fd { open: old as u16 + 1 };
    Ok(new_fd as u16)
}

pub fn dup(slot: usize, fd: u16) -> Result<u16, u64> {
    let _g = IrqLock::acquire(&FS_LOCK);
    dup_into(slot, fd)
}

pub fn dup2(slot: usize, old: u16, new: u16) -> Result<u16, u64> {
    let _g = IrqLock::acquire(&FS_LOCK);
    if old == new {
        open_of(slot, old).ok_or(SYS_ERR_BADF)?;
        return Ok(new);
    }
    let old_idx = open_of(slot, old).ok_or(SYS_ERR_BADF)?;
    if new as usize >= MAX_FDS {
        return Err(SYS_ERR_BADF);
    }
    if let Some(prev) = open_of(slot, new) {
        close_open(prev);
    }
    opens()[old_idx].refs += 1;
    fds(slot)[new as usize] = Fd { open: old_idx as u16 + 1 };
    Ok(new)
}

/// pipe(): returns (read_fd, write_fd). Both ends are refcounted pipes.
pub fn pipe(slot: usize) -> Result<(u16, u16), u64> {
    let _g = IrqLock::acquire(&FS_LOCK);
    let free_fds = (0..MAX_FDS).filter(|&i| fds(slot)[i].open == 0).count();
    if free_fds < 2 {
        return Err(SYS_ERR_MFILE);
    }
    let Some(pid) = pipe::alloc() else { return Err(SYS_ERR_MFILE) };
    let Some(r_open) = alloc_open() else { return Err(SYS_ERR_MFILE) };
    opens()[r_open].used = true; // reserve so the second alloc differs
    let Some(w_open) = alloc_open() else {
        opens()[r_open] = EMPTY_OPEN;
        return Err(SYS_ERR_MFILE)
    };
    pipe::init(pid);
    let rfd = install(slot, r_open, O_RDONLY as u32, 0, pid + 1).ok_or(SYS_ERR_MFILE)?;
    let wfd = install(slot, w_open, O_WRONLY as u32, 0, pid + 1).ok_or(SYS_ERR_MFILE)?;
    Ok((rfd, wfd))
}

/// Node behind an fd (ioctl console checks); None for pipes.
pub fn node_of(slot: usize, fd: u16) -> Option<u16> {
    let _g = IrqLock::acquire(&FS_LOCK);
    let idx = open_of(slot, fd)?;
    if opens()[idx].pipe != 0 {
        return None;
    }
    Some(opens()[idx].node)
}
