//! `cat` — print a file from the mounted filesystems (DESIGN.md §10).
//! Split out of cmds.rs to keep every file inside the size rule.

use core::fmt::Write;

use crate::serial::Serial;
use crate::vfs;

use super::Shell;

/// Scratch buffer for `cat` (4 KiB cap keeps the stack small).
static mut CAT_BUF: [u8; 4096] = [0; 4096];

macro_rules! out {
    ($s:expr, $($arg:tt)*) => {
        { let _ = writeln!($s, $($arg)*); }
    };
}

/// Resolve an 8.3 FAT path (silent on parse failure, so the ext4 path can
/// try the same name).
fn fat_lookup(vfs: &vfs::Vfs, path: &[u8]) -> Option<(u32, u32)> {
    let mut names = [[0u8; 11]; 4];
    let mut refs: [&[u8; 11]; 4] = [&[0; 11]; 4];
    let mut n = 0usize;
    for comp in path.split(|&b| b == b'/') {
        if comp.is_empty() {
            continue;
        }
        if n == names.len() {
            return None;
        }
        let text = core::str::from_utf8(comp).ok()?;
        names[n] = vfs::to_8_3(text)?;
        n += 1;
    }
    if n == 0 {
        return None;
    }
    for i in 0..n {
        refs[i] = &names[i];
    }
    vfs::find_path(&vfs.fs, vfs.fs.root_cluster, &refs[..n])
}

/// Print a file with the non-printable-bytes-filtered dump used by `cat`.
fn dump_ascii(s: &mut Serial, data: &[u8], declared: usize) {
    if declared > data.len() {
        out!(s, "cat: {} bytes, truncated to {}", declared, data.len());
    } else {
        out!(s, "cat: {} bytes", data.len());
    }
    for &b in data {
        let c = if (0x20..0x7F).contains(&b) || b == b'\n' || b == b'\t' { b } else { b'.' };
        let _ = s.write(&[c]);
    }
    if !data.is_empty() && data[data.len() - 1] != b'\n' {
        let _ = s.write(b"\r\n");
    }
}

pub fn cmd_cat(sh: &mut Shell, s: &mut Serial, args: &[&[u8]]) {
    if args.len() != 1 {
        out!(s, "usage: cat <path>   (e.g. cat /HELLO.TXT or cat /etc/fstab)");
        return;
    }
    let Some(vfs) = sh.vfs else {
        out!(s, "cat: no filesystem mounted");
        return;
    };
    let path = args[0];
    let path = if path.first() == Some(&b'/') { &path[1..] } else { path };
    let buf = unsafe { &mut *core::ptr::addr_of_mut!(CAT_BUF) };

    // FAT first (8.3 names), then the ext4 root (case-sensitive long names).
    if let Some((cluster, size)) = fat_lookup(&vfs, path) {
        let want = (size as usize).min(buf.len());
        match vfs.fs.read_file(cluster, want as u32, buf) {
            Some(got) => dump_ascii(s, &buf[..got], size as usize),
            None => out!(s, "cat: read failed"),
        }
        return;
    }
    if let Some(root) = vfs.root.as_ref() {
        let Some(inode) = root.lookup(path) else {
            out!(s, "cat: not found");
            return;
        };
        if !inode.is_file() {
            out!(s, "cat: not a regular file");
            return;
        }
        let want = (inode.size as usize).min(buf.len());
        match root.read_file(&inode, &mut buf[..want]) {
            Some(got) => dump_ascii(s, &buf[..got], inode.size as usize),
            None => out!(s, "cat: read failed"),
        }
        return;
    }
    out!(s, "cat: not found");
}
