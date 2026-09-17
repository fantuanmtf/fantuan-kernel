//! NVRAM / firmware-settings diagnosis (M7.5, DESIGN.md §9): Secure Boot
//! state, setup mode, BootOrder + Boot#### with stale-entry detection against
//! the mounted ESP, and the firmware clock — all read-only. This module owns
//! the shared Boot#### model and the namespace reads; the device-path parse
//! lives in nvram_boot.rs, the report in nvram_report.rs and the repair
//! actions in nvram_repair.rs.
//!
//! Firmware behavior note (verified against OVMF): once the variable policy
//! locks at ReadyToBoot, SetVariable accepts only the boot variables
//! (BootOrder / Boot#### / ...) — arbitrary new names get
//! EFI_INVALID_PARAMETER. Runtime NV writes need an SMM firmware build
//! (tools/run.sh --smm); the non-SMM build rejects them at the platform layer.

use crate::runtime::{Runtime, GLOBAL_GUID};
use crate::vfs::Vfs;

use super::nvram_boot::parse_entry;

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

/// Collect BootOrder + every Boot#### from the variable namespace (the
/// firmware-issued names/vendors are authoritative — direct-name reads are
/// unreliable on some firmware). Returns (entries filled, BootOrder bytes,
/// BootOrder length).
///
/// The scan must visit the WHOLE namespace: stopping early once 8 Boot####
/// entries were seen could miss BootOrder (firmware enumeration order is not
/// alphabetical), and a truncated BootOrder is how the repair path would
/// delete boot entries it never saw. Extra Boot#### names beyond the entry
/// array are ignored; the iteration cap guards against firmware that loops.
pub(super) fn collect(rt: &Runtime, vfs: &Vfs, entries: &mut [BootEntry; 8]) -> (usize, [u8; 64], usize) {
    const MAX_VARIABLES: usize = 256;
    let mut order = [0u8; 64];
    let mut order_n = 0usize;
    let mut names: [[u8; 8]; 8] = [[0; 8]; 8];
    let mut found = 0usize;
    let mut iters = 0usize;
    let mut name_buf = [0u16; 32];
    let mut vendor = GLOBAL_GUID;
    while rt.next_variable(&mut name_buf, &mut vendor) {
        iters += 1;
        if iters > MAX_VARIABLES {
            break;
        }
        let mut ascii = [0u8; 16];
        let alen = utf16_to_ascii(&name_buf, &mut ascii);
        if alen == 9 && &ascii[..9] == b"BootOrder" {
            order_n = rt.get_variable(&name_buf, &vendor, &mut order).unwrap_or(0);
        } else if alen == 8 && &ascii[..4] == b"Boot" && found < names.len() {
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
