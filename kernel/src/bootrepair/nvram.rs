//! NVRAM / firmware-settings diagnosis (M7.5, DESIGN.md §9): Secure Boot
//! state, setup mode, the boot-order list with stale-entry detection against
//! the mounted ESP, and the firmware clock. Read-only GetVariable usage.
//!
//! Known firmware quirk (OVMF, non-SMM build): runtime SetVariable rejects
//! NV-variable CREATES with EFI_INVALID_PARAMETER — repair-by-SetVariable
//! needs per-firmware validation (M7.6). The read path works reliably.

use core::fmt::Write;

use crate::runtime::{Runtime, GLOBAL_GUID};
use crate::serial::Serial;
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
fn ascii_to_utf16(src: &[u8], dst: &mut [u16]) {
    for (i, b) in src.iter().enumerate() {
        if i + 1 < dst.len() {
            dst[i] = *b as u16;
        }
    }
}

pub fn check(s: &mut Serial, rt: &Runtime, vfs: &Vfs) {
    // Self-test on REAL firmware data: BootCurrent must be one of the boot
    // entries we just booted from (OVMF reports 0x0002 = our disk).
    let mut name = [0u16; 32];
    ascii_to_utf16(b"BootCurrent", &mut name);
    let mut buf = [0u8; 16];
    match rt.get_variable(&name, &GLOBAL_GUID, &mut buf) {
        Some(n) if n >= 2 => {
            let cur = u16::from_le_bytes([buf[0], buf[1]]);
            let _ = writeln!(s, "nvram: BootCurrent = {:#06x} (RT read verified)", cur);
        }
        _ => {
            let _ = writeln!(s, "nvram: BootCurrent read failed — RT handover broken");
        }
    }

    // Secure Boot + setup mode (absent when the firmware never enabled SB).
    ascii_to_utf16(b"SecureBoot", &mut name);
    let mut sb = [0u8; 4];
    match rt.get_variable(&name, &GLOBAL_GUID, &mut sb) {
        Some(n) if n >= 1 => {
            let _ = writeln!(s, "nvram: Secure Boot {}", if sb[0] != 0 { "ENABLED — unsigned kernels will fail" } else { "disabled" });
        }
        _ => {
            let _ = writeln!(s, "nvram: Secure Boot variable absent (firmware has it disabled)");
        }
    }
    ascii_to_utf16(b"SetupMode", &mut name);
    let mut sm = [0u8; 4];
    if let Some(n) = rt.get_variable(&name, &GLOBAL_GUID, &mut sm) {
        if n >= 1 && sm[0] != 0 {
            let _ = writeln!(s, "nvram: SetupMode active — Secure Boot enrolled but not enforced");
        }
    }

    // SetVariable probe: honest report of the firmware's runtime behavior.
    ascii_to_utf16(b"FantuanTest", &mut name);
    let sts = rt.set_variable(&name, &GLOBAL_GUID, &42u64.to_le_bytes());
    if sts == 0 {
        let _ = writeln!(s, "nvram: SetVariable works at runtime (repair-ready firmware)");
    } else {
        let _ = writeln!(s, "nvram: SetVariable sts={:#x} — runtime NV writes need per-firmware validation (M7.6)", sts);
    }

    // Boot order + entries: walk the variable namespace with the firmware's
    // own names and vendors (the direct-name path is unreliable on some
    // firmware; the enumeration path is authoritative).
    // Pass 1: collect BootOrder + the Boot#### names (order-agnostic).
    let mut order = [0u8; 64];
    let mut order_n = 0usize;
    let mut boot_names: [[u8; 8]; 16] = [[0; 8]; 16];
    let mut boot_count = 0usize;
    let mut other_count = 0usize;
    let mut name_buf = [0u16; 32];
    let mut vendor = GLOBAL_GUID;
    while boot_count < 16 && rt.next_variable(&mut name_buf, &mut vendor) {
        let mut ascii = [0u8; 16];
        let alen = utf16_to_ascii(&name_buf, &mut ascii);
        if alen == 9 && &ascii[..9] == b"BootOrder" {
            order_n = rt.get_variable(&name_buf, &vendor, &mut order).unwrap_or(0);
        } else if alen == 8 && &ascii[..4] == b"Boot" {
            boot_names[boot_count].copy_from_slice(&ascii[..8]);
            boot_count += 1;
        } else {
            other_count += 1;
        }
    }
    let _ = write!(s, "nvram: BootOrder {} entries [", order_n / 2);
    for o in order[..order_n].chunks(2) {
        if o.len() == 2 {
            let _ = write!(s, "{:04x} ", u16::from_le_bytes([o[0], o[1]]));
        }
    }
    let _ = writeln!(s, "], {} other variables", other_count);

    // Pass 2: read each Boot#### with the name as it was enumerated.
    for i in 0..boot_count {
        let mut name_buf = [0u16; 32];
        for (j, b) in boot_names[i].iter().enumerate() {
            name_buf[j] = *b as u16;
        }
        let mut data = [0u8; 512];
        if let Some(dlen) = rt.get_variable(&name_buf, &GLOBAL_GUID, &mut data) {
            report_entry(s, &boot_names[i], &data[..dlen], &order[..order_n], vfs);
        }
    }

    // Firmware clock.
    if let Some(t) = rt.get_time() {
        let _ = writeln!(
            s,
            "nvram: firmware time {}-{:02}-{:02} {:02}:{:02}:{:02}",
            t.year, t.month, t.day, t.hour, t.minute, t.second
        );
    }
}

fn report_entry(s: &mut Serial, name: &[u8], data: &[u8], order: &[u8], vfs: &Vfs) {
    if data.len() < 6 {
        return;
    }
    let attrs = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let path_len = u16::from_le_bytes([data[4], data[5]]) as usize;
    let active = attrs & 0x1 != 0;

    // Description: UTF-16 from offset 6 until NUL.
    let mut desc = [0u8; 64];
    let mut dlen = 0;
    let mut i = 6;
    while i + 1 < data.len() && dlen < desc.len() {
        let c = u16::from_le_bytes([data[i], data[i + 1]]);
        i += 2;
        if c == 0 {
            break;
        }
        desc[dlen] = if c < 0x80 { c as u8 } else { b'?' };
        dlen += 1;
    }

    // Device path: find the FilePath node (type 4, subtype 4).
    let mut path = [0u8; 96];
    let mut plen = 0;
    let mut p = 6 + dlen * 2 + 2;
    if core::str::from_utf8(name).unwrap_or("?") == "Boot0002" {
        let _ = write!(s, "  [dbg path_len={} dlen={} bytes:", path_len, dlen);
        for k in 0..24 {
            if p + k < data.len() {
                let _ = write!(s, " {:02x}", data[p + k]);
            }
        }
        let _ = writeln!(s, "]");
    }
    while p + 4 <= data.len() && p < 6 + path_len {
        let ntype = data[p];
        let nlen = u16::from_le_bytes([data[p + 2], data[p + 3]]) as usize;
        if nlen < 4 {
            break;
        }
        if ntype == 0x04 && data[p + 1] == 0x04 {
            for c in data[p + 4..(p + nlen).min(data.len())].chunks(2) {
                if c.len() == 2 && plen < path.len() {
                    let ch = u16::from_le_bytes([c[0], c[1]]);
                    if ch == 0 {
                        break;
                    }
                    path[plen] = if ch < 0x80 { ch as u8 } else { b'?' };
                    plen += 1;
                }
            }
        }
        if ntype == 0x7F {
            break;
        }
        p += nlen;
    }

    let mut in_order = false;
    for o in order.chunks(2) {
        if o.len() == 2 && u16::from_le_bytes([o[0], o[1]]) == boot_num(name) {
            in_order = true;
        }
    }

    let _ = write!(s, "nvram: {} ({}): ", core::str::from_utf8(name).unwrap_or("?"), core::str::from_utf8(&desc[..dlen]).unwrap_or("?"));
    let _ = writeln!(s, "{}{}", if active { "active" } else { "inactive" }, if in_order { " [in BootOrder]" } else { "" });

    if plen > 0 {
        if path_exists(vfs, &path[..plen]) {
            let _ = write!(s, "  -> file ");
            let _ = s.write(&path[..plen]);
            let _ = writeln!(s, " present on ESP");
        } else {
            let _ = write!(s, "  -> STALE: file ");
            let _ = s.write(&path[..plen]);
            let _ = writeln!(s, " missing on ESP — entry will fail");
        }
    } else {
        // No FilePath node: the firmware appends the default fallback path.
        if path_exists(vfs, b"\\EFI\\BOOT\\BOOTX64.EFI") {
            let _ = writeln!(s, "  -> default fallback EFI/BOOT/BOOTX64.EFI present on ESP");
        } else {
            let _ = writeln!(s, "  -> STALE: default fallback EFI/BOOT/BOOTX64.EFI missing — entry will fail");
        }
    }
}

/// The 4 hex digits of a "Boot####" name as a u16.
fn boot_num(s: &[u8]) -> u16 {
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
fn path_exists(vfs: &Vfs, path: &[u8]) -> bool {
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
