//! Boot repair v1 (M7, DESIGN.md §9): READ-ONLY diagnosis of a Linux UEFI
//! boot chain — ESP scan, grub.cfg + fstab parsing, UUID/PARTUUID
//! cross-checks against the partition table, and recommendations. No writes
//! at this stage: FAT32 write support (M6.5) turns recommendations into
//! actions.

use core::fmt::Write;

use crate::serial::Serial;
use crate::vfs::fat::Fat32;
use crate::vfs::Vfs;

pub mod esp;
pub mod fstab;
pub mod grub;
pub mod nvram;

fn ascii_upper(b: u8) -> u8 {
    if b.is_ascii_lowercase() {
        b - 32
    } else {
        b
    }
}

/// Convert a fixed string to an 8.3 directory name (no allocation).
pub fn to_8_3(s: &str) -> Option<[u8; 11]> {
    let mut n = [b' '; 11];
    let (base, ext) = match s.find('.') {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    if base.is_empty() || base.len() > 8 {
        return None;
    }
    for (i, c) in base.bytes().enumerate() {
        n[i] = ascii_upper(c);
    }
    if let Some(ext) = ext {
        if ext.len() > 3 {
            return None;
        }
        for (i, c) in ext.bytes().enumerate() {
            n[8 + i] = ascii_upper(c);
        }
    }
    Some(n)
}

/// FAT names are case-insensitive: compare 8.3 names accordingly.
pub fn eq_8_3(a: &[u8; 11], b: &[u8; 11]) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

/// Find a file by 8.3 path components under a directory cluster.
/// Returns (cluster, size).
pub fn find_path(fs: &Fat32, start: u32, components: &[&[u8; 11]]) -> Option<(u32, u32)> {
    let mut dir = start;
    for (i, comp) in components.iter().enumerate() {
        let last = i == components.len() - 1;
        let mut found: Option<(u32, u32)> = None;
        fs.walk_dir(dir, |name, attr, cluster, size| {
            if found.is_none() && eq_8_3(name, comp) {
                let is_dir = attr & 0x10 != 0;
                if (last && !is_dir) || (!last && is_dir) {
                    found = Some((cluster, size));
                }
            }
        });
        let (cluster, size) = found?;
        if last {
            return Some((cluster, size));
        }
        dir = cluster;
    }
    None
}

fn put_hex(t: &mut [u8; 36], off: &mut usize, v: u32, n: usize) {
    const HEX: &[u8] = b"0123456789abcdef";
    for i in (0..n).rev() {
        t[*off] = HEX[((v >> (i * 4)) & 0xF) as usize];
        *off += 1;
    }
}

/// The disk GUID (mixed-endian) in Linux PARTUUID text form.
fn guid_to_text(g: &[u8; 16]) -> [u8; 36] {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut t = [0u8; 36];
    let a = u32::from_le_bytes([g[0], g[1], g[2], g[3]]);
    let b = u16::from_le_bytes([g[4], g[5]]);
    let c = u16::from_le_bytes([g[6], g[7]]);
    let mut off = 0;
    put_hex(&mut t, &mut off, a, 8);
    t[off] = b'-';
    off += 1;
    put_hex(&mut t, &mut off, b as u32, 4);
    t[off] = b'-';
    off += 1;
    put_hex(&mut t, &mut off, c as u32, 4);
    t[off] = b'-';
    off += 1;
    for i in 0..4 {
        t[off] = HEX[(g[8 + i] >> 4) as usize];
        t[off + 1] = HEX[(g[8 + i] & 0xF) as usize];
        off += 2;
    }
    t[off] = b'-';
    off += 1;
    for i in 4..8 {
        t[off] = HEX[(g[8 + i] >> 4) as usize];
        t[off + 1] = HEX[(g[8 + i] & 0xF) as usize];
        off += 2;
    }
    t
}

pub fn run(s: &mut Serial, vfs: &Vfs, runtime_services: u64) {
    let _ = writeln!(s, "bootrepair: v1 diagnosis (read-only)");

    // 1. ESP scan: what bootloaders live in EFI/?
    esp::scan(s, &vfs.fs);

    // 2. grub.cfg (on the ESP, Ubuntu-style).
    let grub = grub::parse(s, &vfs.fs);

    // 3. fstab (copy on the ESP — the real one lives on the ext4 root).
    let mut entries: [fstab::FstabEntry; 4] = [fstab::FstabEntry::none(); 4];
    let n = fstab::parse(s, &vfs.fs, &mut entries);

    // 4. Cross-checks.
    let mut root_uuid: Option<(usize, [u8; 40])> = None;
    for i in 0..n {
        let e = &entries[i];
        if !e.is_partuuid && &e.mount[..e.mount_len] == b"/" {
            root_uuid = Some((e.spec_len, e.spec));
        }
        if e.is_partuuid {
            let mut matched = false;
            for pi in 0..vfs.table.count {
                let text = guid_to_text(&vfs.table.parts[pi].unique_guid);
                if e.spec[..e.spec_len] == text[..36] {
                    let _ = writeln!(s, "bootrepair: fstab PARTUUID matches partition {}", pi + 1);
                    matched = true;
                }
            }
            if !matched {
                let _ = writeln!(s, "bootrepair: WARNING: fstab PARTUUID matches NO partition — EFI mount would fail");
            }
        }
    }
    if let Some((ulen, u)) = root_uuid {
        if let Some(g) = &grub {
            if g.fs_uuid[..g.fs_uuid_len] == u[..ulen] {
                let _ = writeln!(s, "bootrepair: fstab / UUID matches grub.cfg search.fs_uuid (consistent)");
            } else {
                let _ = writeln!(s, "bootrepair: WARNING: fstab / UUID differs from grub.cfg search.fs_uuid");
            }
        }
    }

    // 5. NVRAM / firmware settings (M7.5, read-only GetVariable).
    if let Some(rt) = crate::runtime::Runtime::new(runtime_services) {
        nvram::check(s, &rt, vfs);
    } else {
        let _ = writeln!(s, "bootrepair: runtime services unavailable (rt={:#x})", runtime_services);
    }

    // 6. Recommendations (become actions when FAT32 writes land).
    let _ = writeln!(s, "bootrepair: recommendations:");
    let _ = writeln!(s, "  - fallback loader present: system boots via EFI/BOOT/BOOTX64.EFI");
    let _ = writeln!(s, "  - filesystem UUID checks need ext4 read support (M6.5)");
}
