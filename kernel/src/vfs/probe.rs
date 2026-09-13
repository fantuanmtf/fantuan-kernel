//! M5.5 filesystem type probing (DESIGN.md §8, mount contract clause):
//! "v1 only FAT32 supports actual data read-mounting; all other filesystems
//! are type-probed only, auto ro-mount on non-FAT degrades gracefully with a
//! logged reason + NO mount-table entry." Read-only throughout.

use core::ffi::c_void;
use core::fmt::Write;

use super::part;
use super::{find_path, to_8_3};
use crate::serial::{self, Serial};

extern "C" {
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
}

// --- Filesystem type model ---

#[derive(Copy, Clone, PartialEq)]
pub enum FsType {
    Unknown,
    Fat32,
    Ext4,
    Xfs,
    Btrfs,
    Ntfs,
    Swap,
    /// Best-effort UFS magic is deferred (DESIGN.md §8 lists UFS as
    /// diagnosis-only); the variant keeps the type model complete.
    #[allow(dead_code)]
    Ufs,
}

pub type BootloaderList = [&'static str; 8];

#[derive(Copy, Clone)]
pub struct FsProbeEntry {
    pub part_index: usize,
    pub fstype: FsType,
    pub label: &'static str,
    pub bootloaders: BootloaderList,
    pub mounted: bool,
}

const EMPTY_ENTRY: FsProbeEntry = FsProbeEntry {
    part_index: 0,
    fstype: FsType::Unknown,
    label: "",
    bootloaders: ["", "", "", "", "", "", "", ""],
    mounted: false,
};

const MAX_ENTRIES: usize = 16;
static mut ENTRIES: [FsProbeEntry; MAX_ENTRIES] = [EMPTY_ENTRY; MAX_ENTRIES];
static mut COUNT: usize = 0;

fn fs_name(t: FsType) -> &'static str {
    match t {
        FsType::Fat32 => "FAT32",
        FsType::Ext4 => "ext4",
        FsType::Xfs => "XFS",
        FsType::Btrfs => "Btrfs",
        FsType::Ntfs => "NTFS",
        FsType::Swap => "swap",
        FsType::Ufs => "UFS",
        FsType::Unknown => "unknown",
    }
}

/// Human label for the probe table. Non-FAT filesystems are probe-only in
/// v1 — the label says so explicitly (mount contract).
fn label_for(t: FsType, is_esp: bool, mounted: bool) -> &'static str {
    if is_esp {
        return "EFI System Partition";
    }
    match t {
        FsType::Fat32 if mounted => "FAT32 (mounted ro)",
        FsType::Fat32 => "FAT32",
        FsType::Ext4 => "ext4 (probe-only, v1 no read)",
        FsType::Xfs => "XFS (probe-only, v1 no read)",
        FsType::Btrfs => "Btrfs (probe-only, v1 no read)",
        FsType::Ntfs => "NTFS (probe-only, v1 no read)",
        FsType::Swap => "swap (probe-only, v1 no read)",
        FsType::Ufs => "UFS (probe-only, v1 no read)",
        FsType::Unknown => "unknown",
    }
}

// --- Sector helpers ---

fn read_sector(lba: u64, buf: &mut [u8; 512]) -> bool {
    unsafe { blk_read(core::ptr::null_mut(), lba, buf.as_mut_ptr() as *mut c_void, 1) == 0 }
}

fn read_range(part_lba: u64, first_sector: u64, sectors: usize, out: &mut [u8]) -> bool {
    for s in 0..sectors {
        let mut buf = [0u8; 512];
        if !read_sector(part_lba + first_sector + s as u64, &mut buf) {
            return false;
        }
        let start = s * 512;
        if start + 512 > out.len() {
            return false;
        }
        out[start..start + 512].copy_from_slice(&buf);
    }
    true
}

// --- Magic matching (offsets relative to the partition start) ---

pub fn detect_fs(part: &part::Partition) -> FsType {
    let mut first4k = [0u8; 4096];
    if !read_range(part.first_lba, 0, 8, &mut first4k) {
        return FsType::Unknown;
    }

    // FAT32: BPB filesystem-type string at 0x52 (hand-built images may omit
    // it — fall back to a full BPB parse).
    if &first4k[0x52..0x52 + 8] == b"FAT32   " || super::fat::parse(part.first_lba).is_some() {
        return FsType::Fat32;
    }
    // ext2/3/4: superblock at 1024, magic 0xEF53 at superblock+0x38.
    if first4k[0x438] == 0x53 && first4k[0x439] == 0xEF {
        return FsType::Ext4;
    }
    // XFS: "XFSB" at 0.
    if &first4k[0..4] == b"XFSB" {
        return FsType::Xfs;
    }
    // NTFS: jump + OEM ID.
    if first4k[0] == 0xEB && first4k[2] == 0x90 && &first4k[3..11] == b"NTFS    " {
        return FsType::Ntfs;
    }
    // Swap: page signature at 4086 (v1) or whole-disk "SWAP-SPACE" (v0).
    if &first4k[4086..4096] == b"SWAPSPACE2" || &first4k[0..10] == b"SWAP-SPACE" {
        return FsType::Swap;
    }
    // Btrfs: primary superblock at 64 KiB, magic at +0x40.
    let mut sec = [0u8; 512];
    if read_sector(part.first_lba + 128, &mut sec) && &sec[0x40..0x48] == b"_BHRfS_M" {
        return FsType::Btrfs;
    }
    FsType::Unknown
}

// --- Bootloader enumeration on a FAT partition (read-only) ---

/// Known ESP payloads, in report order: (display path, 8.3 components).
const BOOTLOADER_PATHS: [(&str, [&str; 4]); 8] = [
    ("EFI/BOOT/BOOTX64.EFI", ["EFI", "BOOT", "BOOTX64.EFI", ""]),
    ("EFI/ubuntu/shimx64.efi", ["EFI", "ubuntu", "shimx64.efi", ""]),
    ("EFI/ubuntu/grubx64.efi", ["EFI", "ubuntu", "grubx64.efi", ""]),
    ("EFI/ubuntu/grub.cfg", ["EFI", "ubuntu", "grub.cfg", ""]),
    ("EFI/debian/grubx64.efi", ["EFI", "debian", "grubx64.efi", ""]),
    ("EFI/systemd/systemd-bootx64.efi", ["EFI", "systemd", "systemd-bootx64.efi", ""]),
    ("EFI/Microsoft/Boot/bootmgfw.efi", ["EFI", "Microsoft", "Boot", "bootmgfw.efi"]),
    ("EFI/fantuan/kernel.bin", ["EFI", "fantuan", "kernel.bin", ""]),
];

pub fn scan_bootloaders(fs: &super::fat::Fat32) -> BootloaderList {
    let mut out: BootloaderList = ["", "", "", "", "", "", "", ""];
    let mut n = 0usize;
    for (display, comps) in BOOTLOADER_PATHS.iter() {
        let mut names: [[u8; 11]; 4] = [[0; 11]; 4];
        let mut refs: [&[u8; 11]; 4] = [&[0; 11]; 4];
        let mut used = 0usize;
        for (i, c) in comps.iter().enumerate() {
            if c.is_empty() {
                break;
            }
            let Some(name) = to_8_3(c) else { break };
            names[i] = name;
            used = i + 1;
        }
        if used == 0 {
            continue;
        }
        for i in 0..used {
            refs[i] = &names[i];
        }
        if find_path(fs, fs.root_cluster, &refs[..used]).is_some() && n < out.len() {
            out[n] = display;
            n += 1;
        }
    }
    out
}

// --- Probe-table construction ---

pub fn probe_table() -> &'static [FsProbeEntry] {
    unsafe {
        let base = core::ptr::addr_of!(ENTRIES).cast::<FsProbeEntry>();
        core::slice::from_raw_parts(base, COUNT)
    }
}

/// Probe every partition. MOUNTED_INDEX is the FAT32 partition vfs::init
/// actually mounted (only that one may claim a mount-table entry).
pub unsafe fn init(table: &part::Table, mounted_index: usize) {
    let mut s = Serial::new(serial::COM1);
    let mut count = 0usize;
    for (pi, p) in table.parts[..table.count].iter().enumerate() {
        if count >= MAX_ENTRIES {
            break;
        }
        let fst = detect_fs(p);
        let is_esp = p.type_guid == part::FAT32_GPT_GUID;
        let mounted = fst == FsType::Fat32 && pi == mounted_index;
        let mut bootloaders: BootloaderList = ["", "", "", "", "", "", "", ""];
        if fst == FsType::Fat32 {
            if let Some(fs) = super::fat::parse(p.first_lba) {
                bootloaders = scan_bootloaders(&fs);
            }
        }
        ENTRIES[count] = FsProbeEntry {
            part_index: pi,
            fstype: fst,
            label: label_for(fst, is_esp, mounted),
            bootloaders,
            mounted,
        };
        if fst != FsType::Fat32 && fst != FsType::Unknown {
            let _ = writeln!(
                s,
                "probe: part {} {} identified, not mounted (v1 read-only probe-only)",
                pi + 1,
                fs_name(fst)
            );
        }
        count += 1;
    }
    COUNT = count;
}
