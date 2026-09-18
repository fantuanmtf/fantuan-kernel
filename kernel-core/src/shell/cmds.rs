//! Shared shell command implementations (DESIGN.md §10): help, filesystem
//! listings, mount aliases, bootinfo and the boot-repair entry points.
//! Read-only except `grub-fix`, whose repair/install paths enable repair
//! mode only after an explicit YES confirmation. The x86-only commands
//! (hwdiag, lsdev, diskhealth, crypto-selftest) live in the kernel crate.

use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::diag;
use crate::drv;
use crate::log::Log;
use crate::vfs;

use super::Shell;

macro_rules! out {
    ($s:expr, $($arg:tt)*) => {
        { let _ = writeln!($s, $($arg)*); }
    };
}

/// Surface-scan cancellation flag (set by the progress callback on 'q').
static SCAN_CANCEL: AtomicBool = AtomicBool::new(false);

/// Disk health: driver-decoded identity plus SMART when the transport has
/// it. virtio has no ATA/NVMe SMART, and that is reported explicitly instead
/// of formatting zeroed fields (M9.4-5).
pub fn cmd_diskhealth(_sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    let dev = drv::drive_handle();
    let Some(id) = diag::diskhealth::identify_strings(dev) else {
        out!(s, "diskhealth: IDENTIFY unavailable");
        return;
    };
    let name = drv::drive_name();
    if name != "ahci" && name != "nvme" {
        let _ = write!(s, "  diskhealth: ");
        for &b in &id.model[..id.model_len] {
            let _ = s.write_char(b as char);
        }
        out!(s, "  SMART unsupported for this transport ({})", name);
        out!(s, "  diskhealth: capacity {} MB ({} sectors)", id.capacity_sectors / 2048, id.capacity_sectors);
        return;
    }
    let (ata, nvme) = diag::diskhealth::smart_report(dev);
    let _ = write!(s, "  diskhealth: ");
    diag::diskhealth::format_line(s, &id, ata.as_ref(), nvme.as_ref());

    if args.iter().any(|a| *a == b"--scan") {
        let cap_sectors = (4u64 << 30) / 512;
        let sectors = id.capacity_sectors.min(cap_sectors);
        if id.capacity_sectors > cap_sectors {
            out!(s, "  scan: capped to 4 GiB ({} sectors of {})", sectors, id.capacity_sectors);
        }
        SCAN_CANCEL.store(false, Ordering::Relaxed);
        out!(s, "  scan: {} sectors, press 'q' to cancel", sectors);
        let mut last_pct = u64::MAX;
        let r = diag::diskhealth::surface_scan(
            dev,
            0,
            sectors,
            |done, total| {
                let pct = done * 100 / total.max(1);
                if pct != last_pct && pct % 10 == 0 {
                    last_pct = pct;
                    let mut w = Log::new();
                    let _ = write!(w, "  scan: {}%\r\n", pct);
                }
                if let Some(b) = crate::input::poll_byte() {
                    if b == b'q' || b == b'Q' {
                        SCAN_CANCEL.store(true, Ordering::Relaxed);
                    }
                }
            },
            &SCAN_CANCEL,
        );
        out!(
            s,
            "  scan: done — {} sectors, slow sectors: {}, read errors: {}",
            r.total_sectors,
            r.slow_sectors,
            r.read_errors
        );
    }
}

pub fn cmd_help(sh: &mut Shell, s: &mut Log, _args: &[&[u8]]) {
    out!(s, "shell commands (DESIGN.md §10):");
    for cmd in sh.commands() {
        let _ = write!(s, "  {:<11} {}\n", cmd.name, cmd.help);
    }
}

pub fn cmd_lsos(_sh: &mut Shell, s: &mut Log, _args: &[&[u8]]) {
    for e in vfs::probe::probe_table() {
        let _ = write!(s, "  part {}: {}", e.part_index + 1, e.label);
        let mut first = true;
        for b in e.bootloaders.iter().filter(|b| !b.is_empty()) {
            if first {
                let _ = write!(s, " [");
                first = false;
            } else {
                let _ = write!(s, ", ");
            }
            let _ = write!(s, "{}", b);
        }
        if !first {
            let _ = write!(s, "]");
        }
        out!(s, "{}", if e.mounted { " — mounted ro" } else { " — not mounted" });
    }
}

pub fn cmd_lsmnt(sh: &mut Shell, s: &mut Log, _args: &[&[u8]]) {
    let Some(vfs) = sh.vfs else {
        out!(s, "lsmnt: no filesystem mounted");
        return;
    };
    out!(s, "  /mnt/disk0  part {} (FAT32, ro)", vfs.fat_part + 1);
    if let Some(root) = vfs.root.as_ref() {
        let uuid = crate::vfs::ext4::guid_text(&root.uuid);
        out!(
            s,
            "  /mnt/root0  part {} (ext4, ro, uuid {})",
            vfs.root_part + 1,
            core::str::from_utf8(&uuid).unwrap_or("?")
        );
    }
    for (path, len, active) in sh.mounts().iter() {
        if *active {
            out!(s, "  {}  part {} (ro alias)", core::str::from_utf8(&path[..*len]).unwrap_or("?"), vfs.fat_part + 1);
        }
    }
}

pub fn cmd_mount(sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    if args.len() != 2 {
        out!(s, "usage: mount esp0 /mnt/esp0");
        return;
    }
    if args[0] != b"esp0" {
        out!(s, "mount: not supported (v1 ro-only FAT32 aliases; try 'mount esp0 /mnt/esp0')");
        return;
    }
    if sh.vfs.is_none() {
        out!(s, "mount: no FAT32 filesystem available");
        return;
    }
    if sh.mount_alias(args[1]) {
        let _ = write!(s, "mount: ");
        let _ = s.write(args[1]);
        out!(s, " mounted ro (alias)");
    } else {
        out!(s, "mount: alias table full or path already mounted");
    }
}

pub fn cmd_umount(sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    if args.len() != 1 {
        out!(s, "usage: umount <path>");
        return;
    }
    let _ = write!(s, "umount: ");
    let _ = s.write(args[0]);
    if sh.umount_alias(args[0]) {
        out!(s, " removed");
    } else {
        out!(s, " not in the mount table");
    }
}

pub fn cmd_bootinfo(sh: &mut Shell, s: &mut Log, _args: &[&[u8]]) {
    let bi = sh.bi;
    out!(s, "  magic {:#010x}  version {}", bi.magic, bi.version);
    out!(s, "  kernel_base {:#x}  stack_top {:#x}  rsdp {:#x}", bi.kernel_base, bi.stack_top, bi.rsdp);
    out!(s, "  caps {:#x}  runtime_services {:#x}  smbios_table {:#x}", bi.caps, bi.runtime_services, bi.smbios_table);
    out!(s, "  fb {}x{} stride {} format {}", bi.fb.width, bi.fb.height, bi.fb.stride, bi.fb.format);
    out!(s, "  memmap entries {} (desc {} bytes)", bi.memmap.count, bi.memmap.desc_size);
    out!(s, "  page tables: pml4 {:#x}, {} pages", bi.boot_pml4, bi.boot_tables_pages);
}

pub fn cmd_grubfix(sh: &mut Shell, s: &mut Log, args: &[&[u8]]) {
    let Some(vfs) = sh.vfs else {
        out!(s, "grub-fix: no filesystem mounted");
        return;
    };
    // `install` (M7.9) regenerates the configuration; `repair` applies the
    // ESP/NVRAM fixes; no argument is the read-only diagnosis.
    if args.first().map(|a| *a == b"install").unwrap_or(false) {
        out!(s, "WARNING: install regenerates the boot configuration (GRUBCFG.BAK first) — confirm by typing YES");
        let _ = write!(s, "confirm> ");
        if !sh.read_line(s) || sh.line_bytes() != b"YES" {
            out!(s, "grub-fix: confirmation not YES — aborted (nothing written)");
            return;
        }
        crate::vfs::enable_repair_mode();
        out!(s, "grub-fix: repair mode ON — installing the generated configuration");
        crate::bootrepair::install::run(s, &vfs, sh.rt);
        return;
    }
    let repair = args.first().map(|a| *a == b"repair").unwrap_or(false);
    if !repair {
        out!(s, "grub-fix: diagnosis (read-only) — 'grub-fix repair|install' to act");
        crate::bootrepair::diagnose(s, &vfs, sh.rt);
        return;
    }

    out!(s, "WARNING: repair mode enables disk writes — confirm by typing YES");
    out!(s, "repair actions: ESP fallback copy, FIXED.TXT self-test, NVRAM BootOrder repair");
    let _ = write!(s, "confirm> ");
    if !sh.read_line(s) {
        out!(s, "grub-fix: no confirmation — aborted");
        return;
    }
    if sh.line_bytes() != b"YES" {
        out!(s, "grub-fix: confirmation not YES — aborted (nothing written)");
        return;
    }
    crate::vfs::enable_repair_mode();
    out!(s, "grub-fix: repair mode ON — applying fixes");
    crate::bootrepair::repair(s, &vfs, sh.rt);
}
