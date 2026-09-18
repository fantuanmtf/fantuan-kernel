//! /boot inventory + os-release/default-grub parsing from the mounted ext4
//! root (M7.9, DESIGN.md §9.1). Read-only: the data feeds the grub.cfg
//! generator and the diagnosis report.

use core::fmt::Write;

use crate::log::Log;
use crate::vfs::ext4::Ext4;

/// Image file names live in fixed buffers: the kernel stack cannot hold a
/// generic allocator, and 48 bytes covers every realistic vmlinuz/initrd.
const NAME_MAX: usize = 48;
const MAX_IMAGES: usize = 4;

#[derive(Clone, Copy)]
pub struct ImageName {
    pub name: [u8; NAME_MAX],
    pub len: usize,
}

pub struct Inventory {
    pub boot_found: bool,
    pub kernels: [ImageName; MAX_IMAGES],
    pub kernel_count: usize,
    pub initrds: [ImageName; MAX_IMAGES],
    pub initrd_count: usize,
    pub os_id: [u8; 32],
    pub os_id_len: usize,
    /// GRUB_CMDLINE_LINUX from /etc/default/grub (without quotes).
    pub cmdline: [u8; 128],
    pub cmdline_len: usize,
}

impl Inventory {
    pub fn new() -> Self {
        let empty = ImageName { name: [0; NAME_MAX], len: 0 };
        Inventory {
            boot_found: false,
            kernels: [empty; MAX_IMAGES],
            kernel_count: 0,
            initrds: [empty; MAX_IMAGES],
            initrd_count: 0,
            os_id: [0; 32],
            os_id_len: 0,
            cmdline: [0; 128],
            cmdline_len: 0,
        }
    }
}

/// Read a file by absolute ext4 path into BUF; returns the byte count.
fn read_path(root: &Ext4, path: &[u8], buf: &mut [u8]) -> Option<usize> {
    let inode = root.lookup(path)?;
    if !inode.is_file() {
        return None;
    }
    root.read_file(&inode, buf)
}

/// Extract the unquoted value of `KEY=` from a text buffer.
fn value_of(text: &[u8], key: &[u8]) -> Option<([u8; 32], usize)> {
    for line in text.split(|&b| b == b'\n') {
        let Some(rest) = line.strip_prefix(key) else {
            continue;
        };
        let rest = rest.strip_prefix(b" ").unwrap_or(rest);
        let rest = rest.strip_prefix(b"=").unwrap_or(rest);
        let rest = rest.strip_prefix(b"\"").unwrap_or(rest);
        let end = rest.iter().position(|&b| b == b'"').unwrap_or(rest.len());
        let v = &rest[..end];
        if v.len() > 32 {
            return None;
        }
        let mut out = [0u8; 32];
        out[..v.len()].copy_from_slice(v);
        return Some((out, v.len()));
    }
    None
}

fn push_image(slot: &mut ImageName, name: &[u8]) {
    let n = name.len().min(NAME_MAX);
    slot.name[..n].copy_from_slice(&name[..n]);
    slot.len = n;
}

/// Inventory the mounted ext4 root: /boot contents, os-release ID and the
/// default kernel command line. Logs the findings (and absences).
pub fn scan(s: &mut Log, root: &Ext4) -> Inventory {
    let mut inv = Inventory::new();

    // --- /etc/os-release ---
    let mut buf = [0u8; 1024];
    if let Some(n) = read_path(root, b"/etc/os-release", &mut buf) {
        if let Some((id, id_len)) = value_of(&buf[..n], b"ID") {
            inv.os_id = id;
            inv.os_id_len = id_len;
        }
    }

    // --- /etc/default/grub ---
    let mut gbuf = [0u8; 1024];
    if let Some(n) = read_path(root, b"/etc/default/grub", &mut gbuf) {
        if let Some((cmd, cmd_len)) = value_of(&gbuf[..n], b"GRUB_CMDLINE_LINUX") {
            let n = cmd_len.min(inv.cmdline.len());
            inv.cmdline[..n].copy_from_slice(&cmd[..n]);
            inv.cmdline_len = n;
        }
    }

    // --- /boot ---
    let Some(boot) = root.lookup(b"/boot").filter(|i| i.is_dir()) else {
        let _ = writeln!(s, "  bootfiles: /boot not found on the ext4 root");
        log_summary(s, &inv);
        return inv;
    };
    inv.boot_found = true;
    root.walk_dir(&boot, |name, _ino, _ft| {
        if name.starts_with(b"vmlinuz") && inv.kernel_count < MAX_IMAGES {
            push_image(&mut inv.kernels[inv.kernel_count], name);
            inv.kernel_count += 1;
        } else if (name.starts_with(b"initrd") || name.starts_with(b"initramfs"))
            && inv.initrd_count < MAX_IMAGES
        {
            push_image(&mut inv.initrds[inv.initrd_count], name);
            inv.initrd_count += 1;
        }
    });
    log_summary(s, &inv);
    inv
}

fn log_summary(s: &mut Log, inv: &Inventory) {
    if inv.boot_found {
        let _ = writeln!(
            s,
            "  bootfiles: /boot: {} kernel(s), {} initrd(s)",
            inv.kernel_count, inv.initrd_count
        );
        for i in 0..inv.kernel_count {
            let _ = write!(s, "  bootfiles: kernel ");
            let _ = s.write(&inv.kernels[i].name[..inv.kernels[i].len]);
            let _ = writeln!(s);
        }
        for i in 0..inv.initrd_count {
            let _ = write!(s, "  bootfiles: initrd ");
            let _ = s.write(&inv.initrds[i].name[..inv.initrds[i].len]);
            let _ = writeln!(s);
        }
    }
    if inv.os_id_len > 0 {
        let _ = write!(s, "  bootfiles: os-release ID=");
        let _ = s.write(&inv.os_id[..inv.os_id_len]);
        let _ = writeln!(s);
    }
    if inv.cmdline_len > 0 {
        let _ = write!(s, "  bootfiles: cmdline \"");
        let _ = s.write(&inv.cmdline[..inv.cmdline_len]);
        let _ = writeln!(s, "\"");
    }
}
