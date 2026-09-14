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
pub mod secureboot;

// The FAT 8.3 name helpers are FAT-domain logic and live in vfs (M5.5 probe
// reuses them); re-exported here so the bootrepair submodules keep working.
pub use crate::vfs::{eq_8_3, find_path, to_8_3};

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

/// Read-only diagnosis — the ONLY bootrepair entry point the boot path
/// calls. Nothing here writes to a disk or to NVRAM.
pub fn diagnose(s: &mut Serial, vfs: &Vfs, runtime_services: u64) {
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

    // 6. Recommendations.
    let _ = writeln!(s, "bootrepair: recommendations:");
    let _ = writeln!(s, "  - run 'grub-fix repair' in the shell to apply repairs (YES confirmation)");
}

/// Repair actions — WRITES. Only reachable after the operator explicitly
/// enables repair mode (the shell's `grub-fix repair` + YES, or a future
/// non-interactive opt-in); this function refuses to run otherwise. The boot
/// path calls diagnose() only.
pub fn repair(s: &mut Serial, vfs: &Vfs, runtime_services: u64) {
    if !crate::vfs::repair_mode() {
        let _ = writeln!(s, "repair: refused — repair mode is off (explicit consent required)");
        return;
    }
    let _ = writeln!(s, "repair: repair mode ON — applying fixes");

    // 1. Write self-test: create FIXED.TXT in the root and read it back.
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

    // 2. Fallback-loader repair: when EFI/BOOT/BOOTX64.EFI is missing but
    //    EFI/ubuntu/shimx64.efi exists, stream the latter into place.
    fix_missing_fallback(s, vfs);

    // 3. NVRAM repair (M7.6) + Secure Boot keys (M7.7).
    if let Some(rt) = crate::runtime::Runtime::new(runtime_services) {
        nvram_repair::repair(s, &rt, vfs);
        secureboot::report(s, &rt);
        secureboot::enroll(s, &rt, vfs);
    }
    let _ = writeln!(s, "repair: done");
}

/// EFI/BOOT/BOOTX64.EFI missing + EFI/ubuntu/shimx64.efi present -> stream
/// the shim into place (the classic fallback-loader repair). The copy is
/// chunked, so a real ~1 MiB shim works; a size mismatch aborts rather than
/// publishing a truncated loader.
fn fix_missing_fallback(s: &mut Serial, vfs: &Vfs) {
    let fs = &vfs.fs;
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
    let want = shim_entry.1 as usize;
    if want == 0 {
        let _ = writeln!(s, "repair: shim is empty — nothing to copy");
        return;
    }
    const MAX_FALLBACK: usize = 4 * 1024 * 1024;
    if want > MAX_FALLBACK {
        let _ = writeln!(s, "repair: shim is {} bytes — above the {} byte fallback cap; refused", want, MAX_FALLBACK);
        return;
    }

    // Stream the shim in 32 KiB windows: read a window from the source, append
    // it to the new file, then verify the final size by re-reading the entry.
    let Some(mut w) = fs.create_file(boot_dir, &bootx64) else {
        let _ = writeln!(s, "repair: cannot create EFI/BOOT/BOOTX64.EFI (unsupported cluster size?)");
        return;
    };
    let mut buf = [0u8; 32 * 1024];
    let mut off = 0usize;
    while off < want {
        let n = (want - off).min(buf.len());
        let Some(got) = fs.read_range(shim_entry.0, off as u64, &mut buf[..n]) else {
            let _ = writeln!(s, "repair: fallback copy aborted — source read failed at {}", off);
            return;
        };
        if got == 0 || !w.append(&buf[..got]) {
            let _ = writeln!(s, "repair: fallback copy aborted — write failed at {}", off);
            return;
        }
        off += got;
    }
    let written = w.size();
    if !w.finish() {
        let _ = writeln!(s, "repair: fallback copy failed while publishing the directory entry");
        return;
    }
    // Verify: the directory entry must report exactly the source size.
    match find_path(fs, fs.root_cluster, &[&efi, &boot, &bootx64]) {
        Some((_, size)) if size as usize == want && written as usize == want => {
            let _ = writeln!(
                s,
                "repair: copied EFI/ubuntu/shimx64.efi -> EFI/BOOT/BOOTX64.EFI ({} bytes, verified)",
                want
            );
        }
        Some((_, size)) => {
            let _ = writeln!(s, "repair: fallback copy SIZE MISMATCH (source {} vs entry {})", want, size);
        }
        None => {
            let _ = writeln!(s, "repair: fallback copy published but the entry is not found");
        }
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
