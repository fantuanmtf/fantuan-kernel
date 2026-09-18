//! Fallback-loader repair (M7.5b): when EFI/BOOT/BOOTX64.EFI is missing but
//! EFI/ubuntu/shimx64.efi exists, stream the shim into place. Split out of
//! mod.rs to keep every file inside the size rule.

use core::fmt::Write;

use crate::log::Log;
use crate::vfs::fat::Fat32;
use crate::vfs::{eq_8_3, find_path, to_8_3, Vfs};

/// EFI/BOOT/BOOTX64.EFI missing + EFI/ubuntu/shimx64.efi present -> stream
/// the shim into place (the classic fallback-loader repair). The copy is
/// chunked, so a real ~1 MiB shim works; a size mismatch aborts rather than
/// publishing a truncated loader.
pub(crate) fn fix_missing_fallback(s: &mut Log, vfs: &Vfs) {
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

    // Stream the shim in 4 KiB windows: read a window from the source, append
    // it to the new file, then verify the final size by re-reading the entry.
    // This path is reachable only from repair(), which verified repair mode;
    // the token makes that check a compile-time requirement.
    let Some(token) = crate::vfs::repair_guard() else {
        return;
    };
    let Some(mut w) = fs.create_file(boot_dir, &bootx64, &token) else {
        let _ = writeln!(s, "repair: cannot create EFI/BOOT/BOOTX64.EFI (unsupported cluster size?)");
        return;
    };
    let mut buf = [0u8; 4 * 1024];
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
pub(crate) fn find_dir(fs: &Fat32, start: u32, name: &[u8; 11]) -> Option<u32> {
    let mut found = None;
    fs.walk_dir(start, |n, attr, cluster, _| {
        if found.is_none() && eq_8_3(n, name) && attr & 0x10 != 0 {
            found = Some(cluster);
        }
    });
    found
}
