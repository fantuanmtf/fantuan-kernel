//! Writable in-memory tmpfs for the P1 POSIX layer (docs/POSIX_PLAN.md).
//!
//! Fixed static tables, no kernel heap: 32 nodes (8 fixed system nodes plus
//! runtime files/dirs) and a 64 KiB pool holding up to 8 regular files of
//! 8 KiB each. `/dev/console` and `/dev/null` are fixed character nodes;
//! `/`, `/tmp`, `/etc`, `/bin`, `/usr` are writable directories. The pool is
//! bump-allocated and deleting a file does not reclaim its chunk (documented
//! P1 limit). File I/O lives in `tmpfs_file`, re-exported below so callers
//! see one `tmpfs::` surface.
//!
//! Everything here is kernel-side byte paths; the syscall layer
//! (`vfs::posix`) owns user copies and fd semantics.

use core::sync::atomic::{AtomicBool, Ordering};

use fantuan_abi::{
    SYS_ERR_EXIST, SYS_ERR_NAMETOOLONG, SYS_ERR_NOENT, SYS_ERR_NOTDIR, SYS_ERR_NOTEMPTY,
    SYS_ERR_NOSPC, SYS_ERR_PERM,
};

pub const NAME_MAX: usize = 31;
pub const MAX_NODES: usize = 32;
pub const MAX_FILES: usize = 8;
pub const FILE_CAP: usize = 8 * 1024;

pub const ROOT: u16 = 0;
pub const CONSOLE: u16 = 2;
pub const NULL: u16 = 3;

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Dir,
    File,
    Console,
    Null,
}

#[derive(Clone, Copy)]
pub(super) struct Node {
    pub used: bool,
    pub kind: Kind,
    pub parent: u16,
    pub name_len: u8,
    pub name: [u8; NAME_MAX + 1],
    /// Regular-file size / pool offset (unused for other kinds).
    pub size: u32,
    pub off: u32,
}

const EMPTY: Node = Node {
    used: false,
    kind: Kind::Dir,
    parent: 0,
    name_len: 0,
    name: [0; NAME_MAX + 1],
    size: 0,
    off: 0,
};

static mut NODES: [Node; MAX_NODES] = [EMPTY; MAX_NODES];
static INIT: AtomicBool = AtomicBool::new(false);

/// Populate the fixed tree once. Idempotent; kernel boot is single-context.
fn ensure_init() {
    if INIT.swap(true, Ordering::AcqRel) {
        return;
    }
    let set = |id: usize, kind: Kind, parent: u16, name: &[u8]| {
        let mut n = EMPTY;
        n.used = true;
        n.kind = kind;
        n.parent = parent;
        n.name_len = name.len() as u8;
        n.name[..name.len()].copy_from_slice(name);
        unsafe { core::ptr::write(core::ptr::addr_of_mut!(NODES[id]), n) };
    };
    set(0, Kind::Dir, ROOT, b"/");
    set(1, Kind::Dir, ROOT, b"dev");
    set(2, Kind::Console, 1, b"console");
    set(3, Kind::Null, 1, b"null");
    set(4, Kind::Dir, ROOT, b"tmp");
    set(5, Kind::Dir, ROOT, b"etc");
    set(6, Kind::Dir, ROOT, b"bin");
    set(7, Kind::Dir, ROOT, b"usr");
}

pub(super) fn node(id: u16) -> &'static Node {
    unsafe { &*core::ptr::addr_of!(NODES[id as usize]) }
}

pub(super) fn node_mut(id: u16) -> &'static mut Node {
    unsafe { &mut *core::ptr::addr_of_mut!(NODES[id as usize]) }
}

pub fn kind(id: u16) -> Kind {
    ensure_init();
    node(id).kind
}

pub fn parent(id: u16) -> u16 {
    node(id).parent
}

/// Direct children matching `name` (the fixed nodes 0..8 are never reused).
fn find_child(dir: u16, name: &[u8]) -> Option<u16> {
    (0..MAX_NODES as u16).find(|&i| {
        let n = node(i);
        n.used && n.parent == dir && &n.name[..n.name_len as usize] == name
    })
}

fn valid_name(name: &[u8]) -> bool {
    !name.is_empty()
        && name.len() <= NAME_MAX
        && name != b"."
        && name != b".."
        && !name.contains(&b'/')
}

/// Resolve `path` (absolute or relative to `cwd`).
pub fn lookup(cwd: u16, path: &[u8]) -> Result<u16, u64> {
    ensure_init();
    if path.is_empty() {
        return Err(SYS_ERR_NOENT);
    }
    let mut cur = if path[0] == b'/' { ROOT } else { cwd };
    let mut i = 0;
    while i < path.len() {
        while i < path.len() && path[i] == b'/' {
            i += 1;
        }
        if i >= path.len() {
            break;
        }
        let start = i;
        while i < path.len() && path[i] != b'/' {
            i += 1;
        }
        let comp = &path[start..i];
        if comp == b"." {
            continue;
        }
        if comp == b".." {
            cur = parent(cur);
            continue;
        }
        if comp.len() > NAME_MAX {
            return Err(SYS_ERR_NAMETOOLONG);
        }
        if kind(cur) != Kind::Dir {
            return Err(SYS_ERR_NOTDIR);
        }
        cur = find_child(cur, comp).ok_or(SYS_ERR_NOENT)?;
    }
    Ok(cur)
}

/// Create a child; fixed nodes 0..8 may be parents but never children.
pub fn create(parent_id: u16, name: &[u8], want: Kind) -> Result<u16, u64> {
    ensure_init();
    if want != Kind::Dir && want != Kind::File {
        return Err(SYS_ERR_PERM); // Console/Null are fixed-only
    }
    if kind(parent_id) != Kind::Dir {
        return Err(SYS_ERR_NOTDIR);
    }
    if !valid_name(name) {
        return Err(SYS_ERR_NAMETOOLONG);
    }
    if find_child(parent_id, name).is_some() {
        return Err(SYS_ERR_EXIST);
    }
    if want == Kind::File && count_files() >= MAX_FILES {
        return Err(SYS_ERR_NOSPC);
    }
    let idx = (0..MAX_NODES as u16)
        .find(|&i| !node(i).used)
        .ok_or(SYS_ERR_NOSPC)?;
    let mut n = EMPTY;
    n.used = true;
    n.kind = want;
    n.parent = parent_id;
    n.name_len = name.len() as u8;
    n.name[..name.len()].copy_from_slice(name);
    if want == Kind::File {
        n.off = super::tmpfs_file::reserve()?;
    }
    *node_mut(idx) = n;
    Ok(idx)
}

fn count_files() -> usize {
    (0..MAX_NODES as u16).filter(|&i| node(i).used && node(i).kind == Kind::File).count()
}

fn has_children(id: u16) -> bool {
    (0..MAX_NODES as u16).any(|i| node(i).used && i != id && node(i).parent == id)
}

/// Remove an empty dir or a regular file; the fixed nodes 0..8 are protected.
pub fn remove(id: u16) -> Result<(), u64> {
    ensure_init();
    let n = node(id);
    if !n.used || id <= 7 {
        return Err(SYS_ERR_PERM);
    }
    if n.kind == Kind::Dir && has_children(id) {
        return Err(SYS_ERR_NOTEMPTY);
    }
    node_mut(id).used = false;
    Ok(())
}

/// Move `id` under `new_parent` with `new_name` (same-parent rename too).
pub fn rename(id: u16, new_parent: u16, new_name: &[u8]) -> Result<(), u64> {
    ensure_init();
    if !node(id).used || id <= 7 {
        return Err(SYS_ERR_PERM);
    }
    if kind(new_parent) != Kind::Dir {
        return Err(SYS_ERR_NOTDIR);
    }
    if !valid_name(new_name) {
        return Err(SYS_ERR_NAMETOOLONG);
    }
    if let Some(other) = find_child(new_parent, new_name) {
        if other != id {
            return Err(SYS_ERR_EXIST);
        }
    }
    let n = node_mut(id);
    n.parent = new_parent;
    n.name_len = new_name.len() as u8;
    n.name[..new_name.len()].copy_from_slice(new_name);
    Ok(())
}

// File/dir/stat operations live in tmpfs_file; re-exported for one surface.
pub use super::tmpfs_file::{copy_name, dirent_type, dir_count, dir_entry, read, size, stat, truncate, write};
