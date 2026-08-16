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
pub mod nvram_repair;
pub mod nvram_report;

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
        nvram_report::check(s, &rt, vfs);
    } else {
        let _ = writeln!(s, "bootrepair: runtime services unavailable (rt={:#x})", runtime_services);
    }

    // 6. Repair self-test (M7.5b): explicit repair mode, write a file to the
    // root, read it back, verify. The test disk is regenerated each boot.
    crate::vfs::enable_repair_mode();
    let fixed_name = to_8_3("FIXED.TXT").unwrap();
    let content = b"written by bootrepair v1\n";
    if crate::vfs::write_file(&vfs.fs, vfs.fs.root_cluster, &fixed_name, content) {
        let mut back = [0u8; 64];
        if let Some((c, sz)) = find_path(&vfs.fs, vfs.fs.root_cluster, &[&fixed_name]) {
            if let Some(n) = vfs.fs.read_file(c, sz, &mut back) {
                if n == content.len() && &back[..n] == content {
                    let _ = writeln!(s, "repair: FIXED.TXT write+readback ok");
                } else {
                    let _ = writeln!(s, "repair: FIXED.TXT readback MISMATCH");
                }
            } else {
                let _ = writeln!(s, "repair: FIXED.TXT readback failed");
            }
        }
    } else {
        let _ = writeln!(s, "repair: FIXED.TXT write failed");
    }

    // 7. Fallback-loader repair: when EFI/BOOT/BOOTX64.EFI is missing but
    //    EFI/ubuntu/shimx64.efi exists, copy the latter into place.
    fix_missing_fallback(s, &vfs.fs);

    // 8. NVRAM repair (M7.6): BootOrder rebuild + stale-entry deletion +
    //    boot-entry recreation via SetVariable. Runtime NV writes need an
    //    SMM firmware build (tools/run.sh --smm).
    if let Some(rt) = crate::runtime::Runtime::new(runtime_services) {
        nvram_repair::repair(s, &rt, vfs);
    }

    // 9. Recommendations.
    let _ = writeln!(s, "bootrepair: recommendations:");
    let _ = writeln!(s, "  - filesystem UUID checks need ext4 read support (M6.5)");
}

/// EFI/BOOT/BOOTX64.EFI missing + EFI/ubuntu/shimx64.efi present -> copy
/// the shim into place (the classic fallback-loader repair).
fn fix_missing_fallback(s: &mut Serial, fs: &crate::vfs::fat::Fat32) {
    let efi = to_8_3("EFI").unwrap();
    let boot = to_8_3("BOOT").unwrap();
    let bootx64 = to_8_3("BOOTX64.EFI").unwrap();
    let ubuntu = to_8_3("ubuntu").unwrap();
    let shim = to_8_3("SHIMX64.EFI").unwrap();

    // Is the fallback loader already there?
    let fallback = find_path(fs, fs.root_cluster, &[&efi, &boot, &bootx64]);
    if fallback.is_some() {
        return; // nothing to fix
    }
    let _ = writeln!(s, "bootrepair: fallback loader MISSING — checking for a shim to copy");
    // Locate the BOOT directory cluster (for the write target).
    let mut boot_dir: Option<u32> = None;
    fs.walk_dir(find_dir(fs, fs.root_cluster, &efi).unwrap_or(fs.root_cluster), |name, attr, cluster, _| {
        if boot_dir.is_none() && eq_8_3(name, &boot) && attr & 0x10 != 0 {
            boot_dir = Some(cluster);
        }
    });
    let Some(shim_entry) = find_path(fs, fs.root_cluster, &[&efi, &ubuntu, &shim]) else {
        let _ = writeln!(s, "bootrepair: no shim on the ESP — cannot repair the fallback");
        return;
    };
    let Some(boot_dir) = boot_dir else {
        return;
    };
    let mut buf = [0u8; 4096];
    let Some(n) = fs.read_file(shim_entry.0, shim_entry.1.min(buf.len() as u32), &mut buf) else {
        return;
    };
    if crate::vfs::write_file(fs, boot_dir, &bootx64, &buf[..n]) {
        let _ = writeln!(s, "repair: copied EFI/ubuntu/shimx64.efi -> EFI/BOOT/BOOTX64.EFI ({} bytes)", n);
    } else {
        let _ = writeln!(s, "repair: fallback copy failed");
    }
}

/// Find a directory cluster by one component under a directory.
fn find_dir(fs: &Fat32, start: u32, name: &[u8; 11]) -> Option<u32> {
    let mut found = None;
    fs.walk_dir(start, |n, attr, cluster, _| {
        if found.is_none() && eq_8_3(n, name) && attr & 0x10 != 0 {
            found = Some(cluster);
        }
    });
    found
}
