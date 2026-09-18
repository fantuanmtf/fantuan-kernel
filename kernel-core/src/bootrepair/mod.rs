//! Boot repair v1 (M7, DESIGN.md §9): READ-ONLY diagnosis of a Linux UEFI
//! boot chain — ESP scan, grub.cfg + fstab parsing, UUID/PARTUUID
//! cross-checks against the partition table, and recommendations. The real
//! /etc/fstab is read from the ext4 root when one is mounted (M6.5);
//! otherwise the ESP copy is used. Repair actions (M7.5b+) run only after the
//! operator explicitly enables repair mode; the boot path calls diagnose().

use core::fmt::Write;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::log::Log;
use crate::runtime::Runtime;
use crate::vfs::Vfs;

pub mod bootfiles;
pub mod esp;
pub mod fstab;
pub mod grub;
pub mod grubgen;
pub mod install;
pub mod nvram;
pub mod nvram_boot;
pub mod nvram_repair;
pub mod nvram_report;
pub mod secureboot;

mod fallback;

// The fallback-loader copy is shared by diagnose-time helpers and install;
// the x86 code keeps its historical `super::` paths through this re-export.
pub(crate) use fallback::{find_dir, fix_missing_fallback};

// The FAT 8.3 name helpers are FAT-domain logic and live in vfs (M5.5 probe
// reuses them); re-exported here so the bootrepair submodules keep working.
pub use crate::vfs::{eq_8_3, find_path, to_8_3};

/// Optional authenticated-bundle applier (x86-only crypto, M8.1b). Kernels
/// without a crypto stack leave the default, which skips the step.
static AUTH_APPLY: AtomicUsize = AtomicUsize::new(0);

/// Install the authenticated-variable applier (called once at boot by the
/// kernel that owns the crypto stack).
pub fn set_auth_apply(f: fn(&mut Log, &Runtime, &Vfs)) {
    AUTH_APPLY.store(f as usize, Ordering::Release);
}

fn auth_apply(s: &mut Log, rt: &Runtime, vfs: &Vfs) {
    let p = AUTH_APPLY.load(Ordering::Acquire);
    if p == 0 {
        return;
    }
    let f: fn(&mut Log, &Runtime, &Vfs) = unsafe { core::mem::transmute(p) };
    f(s, rt, vfs);
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

/// Read-only diagnosis — the ONLY bootrepair entry point the boot path
/// calls. Nothing here writes to a disk or to NVRAM.
pub fn diagnose(s: &mut Log, vfs: &Vfs, runtime_services: u64) {
    let _ = writeln!(s, "bootrepair: v1 diagnosis (read-only)");

    // 1. ESP scan: what bootloaders live in EFI/?
    esp::scan(s, &vfs.fs);

    // 1.5 /boot inventory from the ext4 root (feeds the M7.9 generator).
    let _inventory = match vfs.root.as_ref() {
        Some(root) => bootfiles::scan(s, root),
        None => {
            let _ = writeln!(s, "bootrepair: no ext4 root — /boot inventory skipped");
            bootfiles::Inventory::new()
        }
    };

    // 2. grub.cfg (on the ESP, Ubuntu-style).
    let grub = grub::parse(s, &vfs.fs);

    // 3. fstab: the real /etc/fstab on the ext4 root when one is mounted
    //    (M6.5), otherwise the copy placed on the ESP.
    let mut entries: [fstab::FstabEntry; 4] = [fstab::FstabEntry::none(); 4];
    let n = match vfs.root.as_ref() {
        Some(root) => fstab::parse_ext4(s, root, &mut entries)
            .unwrap_or_else(|| fstab::parse(s, &vfs.fs, &mut entries)),
        None => fstab::parse(s, &vfs.fs, &mut entries),
    };

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
    // M6.5: the mounted ext4 root's UUID is what search.fs_uuid should name.
    if let (Some(root), Some(g)) = (vfs.root.as_ref(), grub.as_ref()) {
        if g.fs_uuid_len == 36 {
            let text = crate::vfs::ext4::guid_text(&root.uuid);
            if g.fs_uuid[..36] == text {
                let _ = writeln!(s, "bootrepair: grub.cfg search.fs_uuid matches the ext4 root UUID (consistent)");
            } else {
                let _ = writeln!(s, "bootrepair: WARNING: grub.cfg search.fs_uuid differs from the ext4 root UUID");
            }
        }
    }

    // 5. NVRAM / firmware settings (M7.5, read-only GetVariable).
    if let Some(rt) = Runtime::new(runtime_services) {
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
pub fn repair(s: &mut Log, vfs: &Vfs, runtime_services: u64) {
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

    // 3. NVRAM repair (M7.6) + Secure Boot keys (M7.7) + operator bundles.
    if let Some(rt) = Runtime::new(runtime_services) {
        nvram_repair::repair(s, &rt, vfs);
        secureboot::report(s, &rt);
        secureboot::enroll(s, &rt, vfs);
        // M8.1b: authenticated bundles (PK/KEK/db .auth) — x86-only crypto.
        auth_apply(s, &rt, vfs);
    }
    let _ = writeln!(s, "repair: done");
}
