//! Boot#### device-path parsing (M7.6): description, FilePath text, and the
//! ESP-coverage checks (HD node GPT signature vs mounted partitions, PCI node
//! vs the driven storage controller, target-file existence). Split out of
//! nvram.rs to keep every file inside the size rule.

use super::nvram::BootEntry;
use super::{find_path, to_8_3};
use crate::vfs::Vfs;

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
pub(super) fn parse_entry(e: &mut BootEntry, vfs: &Vfs) {
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
            pci_match = bdf == (crate::drv::storage_bdf() & 0xFF);
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
