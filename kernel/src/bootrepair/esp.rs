//! ESP scan (M7, DESIGN.md §9): walk EFI/<vendor>/<file> two levels deep and
//! identify bootloaders. Read-only.

use core::fmt::Write;

use super::{eq_8_3, to_8_3};
use crate::serial::Serial;
use crate::vfs::fat::Fat32;

fn classify(s: &mut Serial, vendor: &[u8], file: &[u8]) {
    match (vendor, file) {
        (b"boot", b"BOOTX64.EFI") => {
            let _ = writeln!(s, "  esp: EFI/BOOT/BOOTX64.EFI — fallback loader");
        }
        (b"ubuntu", b"SHIMX64.EFI") => {
            let _ = writeln!(s, "  esp: EFI/ubuntu/shimx64.efi — Ubuntu shim (Secure Boot chain)");
        }
        (b"ubuntu", b"GRUBX64.EFI") => {
            let _ = writeln!(s, "  esp: EFI/ubuntu/grubx64.efi — Ubuntu GRUB");
        }
        (b"ubuntu", b"GRUB.CFG") => {
            let _ = writeln!(s, "  esp: EFI/ubuntu/grub.cfg — GRUB config on the ESP");
        }
        (b"debian", b"GRUBX64.EFI") => {
            let _ = writeln!(s, "  esp: EFI/debian/grubx64.efi — Debian GRUB");
        }
        (b"systemd", b"SYSTEMD-BOOTX64.EFI") => {
            let _ = writeln!(s, "  esp: EFI/systemd/systemd-bootx64.efi — systemd-boot");
        }
        (b"linux", _) => {
            let _ = writeln!(s, "  esp: EFI/Linux/… — EFI-stub / UKI boot entry (no GRUB involved)");
        }
        (b"refind", b"REFIND_X64.EFI") => {
            let _ = writeln!(s, "  esp: EFI/refind/refind_x64.efi — rEFInd (refind_linux.conf uses long names)");
        }
        (b"microsoft", b"BOOTMGFW.EFI") => {
            let _ = writeln!(s, "  esp: EFI/Microsoft/bootmgfw.efi — Windows (identified, not repaired)");
        }
        _ => {
            let _ = write!(s, "  esp: EFI/");
            let _ = s.write(vendor);
            let _ = write!(s, "/");
            let _ = s.write(file);
            let _ = writeln!(s);
        }
    }
}

pub fn scan(s: &mut Serial, fs: &Fat32) {
    let Some(efi) = to_8_3("EFI") else { return };
    let efi_entry = {
        let mut found: Option<u32> = None;
        fs.walk_dir(fs.root_cluster, |name, attr, cluster, _size| {
            if found.is_none() && eq_8_3(name, &efi) && attr & 0x10 != 0 {
                found = Some(cluster);
            }
        });
        found
    };
    let Some(efi_dir) = efi_entry else {
        let _ = writeln!(s, "  esp: no EFI directory");
        return;
    };

    let mut vendors: [(u32, [u8; 11]); 8] = [(0, [0; 11]); 8];
    let mut n = 0;
    fs.walk_dir(efi_dir, |name, attr, cluster, _size| {
        if n < 8 && attr & 0x10 != 0 && name[0] != b'.' {
            vendors[n] = (cluster, *name);
            n += 1;
        }
    });

    for i in 0..n {
        let (dir, vendor_raw) = vendors[i];
        // vendor name as text (trimmed, lowercased comparison)
        let mut vendor = [0u8; 11];
        let mut vlen = 0;
        for c in vendor_raw {
            if c != b' ' && c != 0 {
                vendor[vlen] = c.to_ascii_lowercase();
                vlen += 1;
            }
        }
        let vendor = &vendor[..vlen];

        let mut files: [([u8; 11], u32); 8] = [([0; 11], 0); 8];
        let mut m = 0;
        fs.walk_dir(dir, |name, _attr, _cluster, size| {
            if m < 8 && name[0] != b'.' && size > 0 {
                files[m] = (*name, size);
                m += 1;
            }
        });
        for j in 0..m {
            let raw = files[j].0;
            // file name as text
            let mut file = [0u8; 12];
            let mut flen = 0;
            for (k, c) in raw.iter().enumerate() {
                if *c != b' ' && *c != 0 {
                    if k == 8 {
                        file[flen] = b'.';
                        flen += 1;
                    }
                    file[flen] = *c;
                    flen += 1;
                }
            }
            classify(s, vendor, &file[..flen]);
        }
    }

    // systemd-boot lives at the ESP root: /loader/entries/*.conf. The 8.3
    // reader can only enumerate short names; long entry names are counted as
    // "present but not enumerable" via the directory presence alone.
    let Some(loader_name) = to_8_3("loader") else { return };
    let mut loader: Option<u32> = None;
    fs.walk_dir(fs.root_cluster, |name, attr, cluster, _size| {
        if loader.is_none() && eq_8_3(name, &loader_name) && attr & 0x10 != 0 {
            loader = Some(cluster);
        }
    });
    let Some(loader) = loader else {
        let _ = writeln!(s, "  esp: /loader/ absent (no systemd-boot config)");
        return;
    };
    let _ = writeln!(s, "  esp: /loader/ present (systemd-boot config directory)");
    let Some(entries_name) = to_8_3("entries") else { return };
    let mut entries: Option<u32> = None;
    fs.walk_dir(loader, |name, attr, cluster, _size| {
        if entries.is_none() && eq_8_3(name, &entries_name) && attr & 0x10 != 0 {
            entries = Some(cluster);
        }
    });
    let Some(entries) = entries else {
        let _ = writeln!(s, "  esp: /loader/entries/ absent");
        return;
    };
    let mut confs = 0usize;
    fs.walk_dir(entries, |name, _attr, _cluster, _size| {
        if &name[8..11] == b"CON" {
            confs += 1;
        }
    });
    let _ = writeln!(s, "  esp: /loader/entries/: {} 8.3 .conf entry(ies)", confs);
}
