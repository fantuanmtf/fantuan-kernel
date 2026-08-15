//! GPT/MBR partition-table parsing (M6, DESIGN.md §8). Pure Rust logic on
//! top of the C block driver — this layer never touches the device directly.

use core::ffi::c_void;
use core::fmt::Write;

use crate::serial::Serial;

extern "C" {
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
}

/// GPT type GUID for FAT32 partitions (EBD0A0A2-B9E5-4433-87C0-68B6B72699C7),
/// as stored on disk (mixed-endian).
pub const FAT32_GPT_GUID: [u8; 16] = [
    0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99, 0xc7,
];

#[derive(Clone, Copy)]
pub struct Partition {
    pub first_lba: u64,
    pub last_lba: u64,
    /// GPT type GUID, or MBR type byte in [0] with the rest zero.
    pub type_guid: [u8; 16],
}

#[derive(PartialEq)]
pub enum TableKind {
    Gpt,
    Mbr,
}

pub struct Table {
    /// GPT vs MBR — consumed by M6.5 boot-repair logic; parsed now.
    #[allow(dead_code)]
    pub kind: TableKind,
    pub parts: [Partition; 8],
    pub count: usize,
}

fn read_sector(lba: u64, buf: &mut [u8; 512]) -> bool {
    unsafe { blk_read(core::ptr::null_mut(), lba, buf.as_mut_ptr() as *mut c_void, 1) == 0 }
}

pub fn parse(s: &mut Serial) -> Option<Table> {
    let mut lba0 = [0u8; 512];
    let mut lba1 = [0u8; 512];
    if !read_sector(0, &mut lba0) || !read_sector(1, &mut lba1) {
        return None;
    }
    if &lba1[..8] == b"EFI PART" {
        parse_gpt(s, &lba1)
    } else if lba0[510] == 0x55 && lba0[511] == 0xAA {
        parse_mbr(s, &lba0)
    } else {
        let _ = writeln!(s, "  part: no partition table found");
        None
    }
}

fn parse_gpt(s: &mut Serial, hdr: &[u8; 512]) -> Option<Table> {
    let entries_lba = u64::from_le_bytes(hdr[72..80].try_into().ok()?);
    let num_entries = u32::from_le_bytes(hdr[80..84].try_into().ok()?) as usize;
    let _ = writeln!(s, "  part: GPT, {} entries at LBA {}", num_entries, entries_lba);

    let mut table = Table {
        kind: TableKind::Gpt,
        parts: [Partition { first_lba: 0, last_lba: 0, type_guid: [0; 16] }; 8],
        count: 0,
    };
    let mut sector = [0u8; 512];
    for i in 0..num_entries.min(8) {
        let byte_off = i * 128;
        let entry_lba = entries_lba + (byte_off / 512) as u64;
        if !read_sector(entry_lba, &mut sector) {
            break;
        }
        let in_sector = byte_off % 512;
        let e = &sector[in_sector..in_sector + 128];
        let type_guid: [u8; 16] = e[0..16].try_into().ok()?;
        if type_guid == [0u8; 16] {
            break; // unused entry: end of the list
        }
        let first = u64::from_le_bytes(e[32..40].try_into().ok()?);
        let last = u64::from_le_bytes(e[40..48].try_into().ok()?);
        table.parts[table.count] = Partition { first_lba: first, last_lba: last, type_guid };
        table.count += 1;
    }
    Some(table)
}

fn parse_mbr(s: &mut Serial, lba0: &[u8; 512]) -> Option<Table> {
    let _ = writeln!(s, "  part: MBR partition table");
    let mut table = Table {
        kind: TableKind::Mbr,
        parts: [Partition { first_lba: 0, last_lba: 0, type_guid: [0; 16] }; 8],
        count: 0,
    };
    for i in 0..4 {
        let e = &lba0[446 + i * 16..446 + i * 16 + 16];
        let type_byte = e[4];
        if type_byte == 0 {
            continue;
        }
        let first = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as u64;
        let sectors = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as u64;
        let mut guid = [0u8; 16];
        guid[0] = type_byte;
        table.parts[table.count] = Partition { first_lba: first, last_lba: first + sectors - 1, type_guid: guid };
        table.count += 1;
    }
    Some(table)
}
