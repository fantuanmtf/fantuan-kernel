//! NVRAM / firmware-settings diagnosis (M7.5, DESIGN.md §9): Secure Boot
//! state, setup mode, BootOrder + Boot#### with stale-entry detection against
//! the mounted ESP, and the firmware clock — all read-only. This module also
//! owns the shared Boot#### model (collect/parse) consumed by the M7.6
//! repair actions in nvram_repair.rs.
//!
//! Firmware behavior note (verified against OVMF): once the variable policy
//! locks at ReadyToBoot, SetVariable accepts only the boot variables
//! (BootOrder / Boot#### / ...) — arbitrary new names get
//! EFI_INVALID_PARAMETER. Runtime NV writes need an SMM firmware build
//! (tools/run.sh --smm); the non-SMM build rejects them at the platform layer.

use crate::runtime::{Runtime, GLOBAL_GUID};
use crate::vfs::Vfs;

use super::{find_path, to_8_3};

/// Copy UTF-16 (until NUL) into an ASCII buffer; returns the length.
fn utf16_to_ascii(src: &[u16], dst: &mut [u8]) -> usize {
    let mut n = 0;
    for &c in src {
        if c == 0 || n >= dst.len() {
            break;
        }
        dst[n] = if c < 0x80 { c as u8 } else { b'?' };
        n += 1;
    }
    n
}

/// ASCII (no-alloc) into a UTF-16 buffer with NUL terminator.
pub(super) fn ascii_to_utf16(src: &[u8], dst: &mut [u16]) {
    for (i, b) in src.iter().enumerate() {
        if i + 1 < dst.len() {
            dst[i] = *b as u16;
        }
    }
}

/// One Boot#### option as parsed from NVRAM.
#[derive(Clone, Copy)]
pub(super) struct BootEntry {
    pub name: [u8; 8],
    pub num: u16,
    pub data: [u8; 512],
    pub dlen: usize,
    pub desc: [u8; 64],
    pub desc_len: usize,
    pub path: [u8; 96],
    pub path_len: usize,
    /// The entry boots the mounted ESP: either its HD node's GPT signature
    /// matches the partition's unique GUID, or it is a whole-disk entry
    /// (no HD node) on the AHCI controller that carries the ESP.
    pub covers_esp: bool,
    /// Partition-level entry: HD node with the ESP's GPT signature. The
    /// explicit entry the repair creates/maintains.
    pub has_partition_match: bool,
    /// The file the entry points at (FilePath node, or the default
    /// \EFI\BOOT\BOOTX64.EFI) exists on the mounted ESP. Only meaningful
    /// when covers_esp.
    pub file_ok: bool,
    pub has_hd_node: bool,
}

impl BootEntry {
    pub(super) const fn none() -> BootEntry {
        BootEntry {
            name: [0; 8],
            num: u16::MAX,
            data: [0; 512],
            dlen: 0,
            desc: [0; 64],
            desc_len: 0,
            path: [0; 96],
            path_len: 0,
            covers_esp: false,
            has_partition_match: false,
            file_ok: false,
            has_hd_node: false,
        }
    }
}

/// The 4 hex digits of a "Boot####" name as a u16.
pub(super) fn boot_num(s: &[u8]) -> u16 {
    if s.len() < 8 {
        return u16::MAX;
    }
    fn hex(b: u8) -> u16 {
        match b {
            b'0'..=b'9' => (b - b'0') as u16,
            b'A'..=b'F' => (b - b'A' + 10) as u16,
            b'a'..=b'f' => (b - b'a' + 10) as u16,
            _ => 0xFFFF,
        }
    }
    (hex(s[4]) << 12) | (hex(s[5]) << 8) | (hex(s[6]) << 4) | hex(s[7])
}

/// Does a Windows-style file path (\EFI\ubuntu\shimx64.efi) exist on the
/// mounted FAT32?
pub(super) fn path_exists(vfs: &Vfs, path: &[u8]) -> bool {
    let bytes: &[u8] = if path.first() == Some(&b'\\') { &path[1..] } else { path };
    let mut comps: [&[u8]; 4] = [&[]; 4];
    let mut n = 0;
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\\' {
            if i > start && n < 4 {
                comps[n] = &bytes[start..i];
                n += 1;
            }
            start = i + 1;
        }
    }
    if start < bytes.len() && n < 4 {
        comps[n] = &bytes[start..];
        n += 1;
    }
    if n == 0 {
        return false;
    }
    let mut names: [[u8; 11]; 4] = [[0; 11]; 4];
    let mut refs: [&[u8; 11]; 4] = [&[0; 11]; 4];
    for i in 0..n {
        let Some(s) = core::str::from_utf8(comps[i]).ok().and_then(to_8_3) else {
            return false;
        };
        names[i] = s;
    }
    for i in 0..n {
        refs[i] = &names[i];
    }
    find_path(&vfs.fs, vfs.fs.root_cluster, &refs[..n]).is_some()
}

/// Parse a Boot#### payload: description, FilePath text, and whether the HD
/// node's GPT signature matches a mounted partition.
fn parse_entry(e: &mut BootEntry, vfs: &Vfs) {
    let data = &e.data[..e.dlen];
    if data.len() < 6 {
        return;
    }
    let path_len = u16::from_le_bytes([data[4], data[5]]) as usize;

    // Description: UTF-16 from offset 6 until NUL.
    let mut i = 6;
    while i + 1 < data.len() && e.desc_len < e.desc.len() {
        let c = u16::from_le_bytes([data[i], data[i + 1]]);
        i += 2;
        if c == 0 {
            break;
        }
        e.desc[e.desc_len] = if c < 0x80 { c as u8 } else { b'?' };
        e.desc_len += 1;
    }

    // Device path nodes until End (0x7F) or path_len is consumed. The
    // length is measured from the start of the device path, right after
    // the description.
    let path_start = 6 + e.desc_len * 2 + 2;
    let mut p = path_start;
    let mut pci_match = false;
    while p + 4 <= data.len() && p < path_start + path_len {
        let ntype = data[p];
        let subtype = data[p + 1];
        let nlen = u16::from_le_bytes([data[p + 2], data[p + 3]]) as usize;
        if nlen < 4 {
            break;
        }
        if ntype == 0x01 && subtype == 0x01 && nlen >= 6 {
            // PCI node: payload = function, device. Direct PciRoot children
            // sit on bus 0, matching storage_bdf() (bus<<8|dev).
            let bdf = data[p + 5] as u32;
            pci_match = bdf == (crate::drivers::storage_bdf() & 0xFF);
        }
        if ntype == 0x04 && subtype == 0x01 && nlen >= 42 {
            // HD node: partition signature at +24 (16 bytes), signature type
            // at +40. Signature type 2 = GPT partition GUID.
            e.has_hd_node = true;
            if data[p + 40] == 0x02 {
                for part in &vfs.table.parts[..vfs.table.count] {
                    if data[p + 24..p + 40] == part.unique_guid {
                        e.covers_esp = true;
                        e.has_partition_match = true;
                    }
                }
            }
        }
        if ntype == 0x04 && subtype == 0x04 {
            for c in data[p + 4..(p + nlen).min(data.len())].chunks(2) {
                if c.len() == 2 && e.path_len < e.path.len() {
                    let ch = u16::from_le_bytes([c[0], c[1]]);
                    if ch == 0 {
                        break;
                    }
                    e.path[e.path_len] = if ch < 0x80 { ch as u8 } else { b'?' };
                    e.path_len += 1;
                }
            }
        }
        if ntype == 0x7F {
            break;
        }
        p += nlen;
    }
    // Whole-disk entry (no HD node) on the AHCI controller that carries the
    // ESP: the firmware loads the default fallback from that disk, so the
    // entry covers our ESP.
    if pci_match && !e.has_hd_node {
        e.covers_esp = true;
    }

    // The entry's target file must exist on the ESP (FilePath node, else the
    // default fallback).
    e.file_ok = if e.path_len > 0 {
        path_exists(vfs, &e.path[..e.path_len])
    } else {
        path_exists(vfs, b"\\EFI\\BOOT\\BOOTX64.EFI")
    };
}

/// Collect BootOrder + every Boot#### from the variable namespace (the
/// firmware-issued names/vendors are authoritative — direct-name reads are
/// unreliable on some firmware). Returns (entries filled, BootOrder bytes,
/// BootOrder length).
pub(super) fn collect(rt: &Runtime, vfs: &Vfs, entries: &mut [BootEntry; 8]) -> (usize, [u8; 64], usize) {
    let mut order = [0u8; 64];
    let mut order_n = 0usize;
    let mut names: [[u8; 8]; 8] = [[0; 8]; 8];
    let mut found = 0usize;
    let mut name_buf = [0u16; 32];
    let mut vendor = GLOBAL_GUID;
    while found < 8 && rt.next_variable(&mut name_buf, &mut vendor) {
        let mut ascii = [0u8; 16];
        let alen = utf16_to_ascii(&name_buf, &mut ascii);
        if alen == 9 && &ascii[..9] == b"BootOrder" {
            order_n = rt.get_variable(&name_buf, &vendor, &mut order).unwrap_or(0);
        } else if alen == 8 && &ascii[..4] == b"Boot" {
            names[found].copy_from_slice(&ascii[..8]);
            found += 1;
        }
    }

    let mut count = 0usize;
    for i in 0..found {
        let mut name_buf = [0u16; 32];
        for (j, b) in names[i].iter().enumerate() {
            name_buf[j] = *b as u16;
        }
        let mut e = BootEntry::none();
        e.name = names[i];
        e.num = boot_num(&names[i]);
        if let Some(dlen) = rt.get_variable(&name_buf, &GLOBAL_GUID, &mut e.data) {
            e.dlen = dlen;
            parse_entry(&mut e, vfs);
        }
        entries[count] = e;
        count += 1;
    }
    (count, order, order_n.min(order.len()))
}

/// Presence/size of a variable through the enumeration path. A too-small
/// buffer still proves existence: the firmware then returns
/// EFI_BUFFER_TOO_SMALL with the required size.
pub(super) fn read_by_name_status(rt: &Runtime, want: &[u8], buf: &mut [u8]) -> Option<usize> {
    let mut name_buf = [0u16; 32];
    let mut vendor = GLOBAL_GUID;
    while rt.next_variable(&mut name_buf, &mut vendor) {
        let mut ascii = [0u8; 16];
        let alen = utf16_to_ascii(&name_buf, &mut ascii);
        if alen == want.len() && &ascii[..alen] == want {
            let (sts, size) = rt.get_variable_status(&name_buf, &vendor, buf);
            if sts == crate::runtime::RT_SUCCESS || sts == crate::runtime::RT_BUFFER_TOO_SMALL {
                return Some(size);
            }
            return None;
        }
    }
    None
}

/// Read a variable by enumerating the namespace and matching the name —
/// the reliable read path (direct-name GetVariable fails on some firmware).
pub(super) fn read_by_name(rt: &Runtime, want: &[u8], buf: &mut [u8]) -> Option<usize> {
    let mut name_buf = [0u16; 32];
    let mut vendor = GLOBAL_GUID;
    while rt.next_variable(&mut name_buf, &mut vendor) {
        let mut ascii = [0u8; 16];
        let alen = utf16_to_ascii(&name_buf, &mut ascii);
        if alen == want.len() && &ascii[..alen] == want {
            return rt.get_variable(&name_buf, &vendor, buf);
        }
    }
    None
}

// The diagnosis report (check) lives in nvram_report.rs — this module owns
// the shared Boot#### model only.
