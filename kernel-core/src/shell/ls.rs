//! `ls` — list a directory on the mounted filesystems (M12-5). The NTFS
//! path under /mnt/win0 is the M12-5 deliverable; the FAT root and the
//! ext4 root are listed for rescue parity. Read-only: there is no unlink/
//! create path here. Split from cat.rs to keep every file inside the size
//! rule.

use core::fmt::Write;

use crate::log::Log;
use crate::vfs;

use super::Shell;

macro_rules! out {
    ($s:expr, $($arg:tt)*) => {
        { let _ = writeln!($s, $($arg)*); }
    };
}

pub fn cmd_ls(sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    let Some(vfs) = sh.vfs else {
        out!(s, "ls: no filesystem mounted");
        return;
    };
    let raw = args.first().copied().unwrap_or(b"/");
    let path = if raw.first() == Some(&b'/') { &raw[1..] } else { raw };
    let shown = core::str::from_utf8(raw).unwrap_or("?");

    #[cfg(kconfig_ntfs)]
    if vfs::ntfs::win_rel(path).is_some() {
        ls_ntfs(&vfs, s, path, shown);
        return;
    }
    // The ext4 root keeps real case (paths like /etc); FAT names are 8.3.
    if let Some(root) = vfs.root.as_ref() {
        if let Some(inode) = root.lookup(path) {
            if inode.is_dir() {
                out!(s, "ls: {} (ext4 ro)", shown);
                root.walk_dir(&inode, |name, _ino, ftype| {
                    out!(s, "  {} {}", if ftype == 2 { "d" } else { "f" }, core::str::from_utf8(name).unwrap_or("?"));
                });
                return;
            }
        }
    }
    if let Some(cluster) = fat_dir(&vfs, path) {
        out!(s, "ls: {} (FAT32 ro)", shown);
        vfs.fs.walk_dir(cluster, |name, attr, _cluster, size| {
            let mut buf = [0u8; 13];
            let n = vfs::fmt_name(name, &mut buf);
            out!(s, "  {} {} {}", if attr & 0x10 != 0 { "d" } else { "f" }, size, core::str::from_utf8(&buf[..n]).unwrap_or("?"));
        });
        return;
    }
    out!(s, "ls: not found");
}

/// Walk an 8.3 path to a FAT directory cluster.
fn fat_dir(vfs: &vfs::Vfs, path: &[u8]) -> Option<u32> {
    let mut cluster = vfs.fs.root_cluster;
    for comp in path.split(|&b| b == b'/') {
        if comp.is_empty() {
            continue;
        }
        let name = vfs::to_8_3(core::str::from_utf8(comp).ok()?)?;
        let mut next = None;
        vfs.fs.walk_dir(cluster, |n, attr, c, _size| {
            if next.is_none() && vfs::eq_8_3(n, &name) && attr & 0x10 != 0 {
                next = Some(c);
            }
        });
        cluster = next?;
    }
    Some(cluster)
}

#[cfg(kconfig_ntfs)]
fn ls_ntfs(vfs: &vfs::Vfs, s: &mut Log, path: &[u8], shown: &str) {
    use crate::vfs::ntfs;
    let Some(win) = vfs.win.as_ref() else {
        out!(s, "ls: NTFS not mounted");
        return;
    };
    let rel = ntfs::win_rel(path).unwrap_or(b"");
    let mut scratch = [0u8; ntfs::MAX_INDEX_BLOCK];
    let rec = match win.resolve(rel, &mut scratch) {
        Ok(r) => r,
        Err(ntfs::NtfsErr::NotFound) => {
            out!(s, "ls: NTFS: not found");
            return;
        }
        Err(e) => {
            out!(s, "ls: NTFS: {}", e.text());
            return;
        }
    };
    if !rec.is_dir() {
        out!(s, "ls: NTFS: not a directory");
        return;
    }
    out!(s, "ls: {} (NTFS ro)", shown);
    if let Err(e) = win.walk_dir(&rec, &mut scratch, &mut |name, _child, is_dir, size| {
        out!(s, "  {} {} {}", if is_dir { "d" } else { "f" }, size, core::str::from_utf8(name).unwrap_or("?"));
        true
    }) {
        out!(s, "ls: NTFS: {}", e.text());
    }
}
