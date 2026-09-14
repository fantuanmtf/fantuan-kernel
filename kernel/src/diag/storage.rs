//! Stage-2 storage diagnostics (DESIGN.md §7/§8.1), in report order:
//!   ① drive identity (IDENTIFY strings)   ② SMART health line
//!   ③ per-partition filesystem types      ④ ESP bootloaders
//! Read-only — nothing here ever writes to a disk.

use core::ffi::c_void;
use core::fmt::Write;

use super::diskhealth;
use super::Severity;
use crate::serial::Serial;
use crate::vfs::probe;

extern "C" {
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
}

pub fn check(s: &mut Serial) -> Severity {
    let dev = crate::drivers::drive_handle();

    // --- ① Boot-header scan + IDENTIFY strings ---
    let mut lba0 = [0u8; 512];
    if unsafe { blk_read(dev, 0, lba0.as_mut_ptr() as *mut c_void, 1) } != 0 {
        let _ = writeln!(s, "  storage: LBA0 read failed");
        return Severity::Critical;
    }
    let is_mbr = lba0[510] == 0x55 && lba0[511] == 0xAA;
    let is_gpt = &lba0[..8] == b"EFI PART";

    let Some(id) = diskhealth::identify_strings(dev) else {
        let _ = writeln!(s, "  storage: IDENTIFY failed");
        return Severity::Warning;
    };
    let _ = writeln!(
        s,
        "  storage: {} (sn {}) — {} sectors, {} MiB",
        core::str::from_utf8(&id.model[..id.model_len]).unwrap_or("?"),
        core::str::from_utf8(&id.serial[..id.serial_len]).unwrap_or("?"),
        id.capacity_sectors,
        id.capacity_sectors / 2048
    );
    let _ = writeln!(s, "  storage: LBA0 boot header: mbr {} gpt {}", is_mbr, is_gpt);

    // --- ② SMART health (ATA attributes or the NVMe health log) ---
    let (ata, nvme) = diskhealth::smart_report(dev);
    let _ = write!(s, "  diskhealth: ");
    diskhealth::format_line(s, &id, ata.as_ref(), nvme.as_ref());
    if ata.is_none() && nvme.is_none() {
        let _ = writeln!(s, "  diskhealth: SMART unavailable on this drive");
    }

    // --- ③ per-partition filesystem types ---
    let table = probe::probe_table();
    let mut probe_only = 0usize;
    if !table.is_empty() {
        let _ = write!(s, "  fs:");
        for e in table {
            let _ = write!(s, " part {} {}", e.part_index + 1, e.label);
            if e.mounted && !e.label.contains("mounted") {
                let _ = write!(s, " (mounted ro)");
            }
            if !e.mounted && e.fstype != probe::FsType::Unknown {
                probe_only += 1;
            }
        }
        let _ = writeln!(s);

        // --- ④ bootloaders found on FAT partitions ---
        let _ = write!(s, "  bootloaders:");
        for e in table {
            for b in e.bootloaders.iter().filter(|b| !b.is_empty()) {
                let _ = write!(s, " {}", b);
            }
        }
        let _ = writeln!(s);
    }

    if probe_only > 0 {
        Severity::Warning
    } else {
        Severity::Ok
    }
}
