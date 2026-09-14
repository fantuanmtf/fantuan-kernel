//! NVRAM repair via SetVariable (M7.6, DESIGN.md §9): rebuild BootOrder,
//! delete stale ESP boot entries, recreate the missing ESP boot entry.
//! Callers must run inside repair mode (the rescue iron rule).
//!
//! Only the boot variables are writable: the firmware's variable policy
//! locks the namespace at ReadyToBoot and accepts SetVariable solely for
//! BootOrder / Boot#### / ... (arbitrary names get EFI_INVALID_PARAMETER).
//! Runtime NV writes additionally need an SMM firmware build.

use core::fmt::Write;

use crate::runtime::{Runtime, GLOBAL_GUID};
use crate::serial::Serial;
use crate::vfs::Vfs;

use super::nvram::{ascii_to_utf16, collect, path_exists, read_by_name, BootEntry};

const NV_BS_RT: u32 = 0x7; // creates/updates at runtime: NV | BS | RT
const LOAD_ACTIVE: u32 = 0x1;

pub(super) fn repair(s: &mut Serial, rt: &Runtime, vfs: &Vfs) {
    let mut entries = [BootEntry::none(); 8];
    let (count, order, order_n) = collect(rt, vfs, &mut entries);

    // 1. New BootOrder: keep every entry that is neither missing nor stale
    //    (targets the ESP but its file is gone); dedup.
    let mut new_order = [0u16; 32];
    let mut new_n = 0usize;
    for o in order[..order_n].chunks(2) {
        if o.len() != 2 {
            break;
        }
        let num = u16::from_le_bytes([o[0], o[1]]);
        let mut exists = false;
        let mut keep = true;
        for e in &entries[..count] {
            if e.num == num {
                exists = true;
                if e.covers_esp && !e.file_ok {
                    keep = false;
                }
                break;
            }
        }
        if !exists {
            keep = false;
        }
        let mut dup = false;
        for i in 0..new_n {
            if new_order[i] == num {
                dup = true;
            }
        }
        if keep && !dup {
            new_order[new_n] = num;
            new_n += 1;
        }
    }

    // 2. Ensure an explicit partition-level entry exists (whole-disk
    //    removable entries are a firmware fallback, not the managed entry).
    //    Only create when the ESP's fallback file is actually there — never
    //    register an entry that would immediately be stale. Idempotent: the
    //    created entry matches by GPT signature on the next boot.
    let mut has_partition_entry = false;
    for e in &entries[..count] {
        if e.has_partition_match {
            has_partition_entry = true;
        }
    }
    if !has_partition_entry && path_exists(vfs, b"\\EFI\\BOOT\\BOOTX64.EFI") {
        if let Some(num) = create_esp_entry(s, rt, vfs, &entries[..count]) {
            if new_n < new_order.len() {
                for i in (0..new_n).rev() {
                    new_order[i + 1] = new_order[i];
                }
                new_order[0] = num;
                new_n += 1;
            } else {
                let _ = writeln!(s, "repair: BootOrder table is full — entry registered but not ordered");
            }
        }
    }

    // 3. Persist BootOrder when it changed, and verify by re-reading.
    let mut flat = [0u8; 64];
    for i in 0..new_n {
        flat[2 * i] = new_order[i] as u8;
        flat[2 * i + 1] = (new_order[i] >> 8) as u8;
    }
    let new_len = new_n * 2;
    if new_len == order_n && flat[..new_len] == order[..order_n] {
        let _ = writeln!(s, "repair: BootOrder unchanged ({} entries)", new_n);
    } else {
        let mut name = [0u16; 32];
        ascii_to_utf16(b"BootOrder", &mut name);
        let sts = rt.set_variable(&name, &GLOBAL_GUID, NV_BS_RT, &flat[..new_len]);
        let mut verify = [0u8; 64];
        let vn = read_by_name(rt, b"BootOrder", &mut verify);
        let ok = sts == 0 && vn == Some(new_len) && verify[..new_len] == flat[..new_len];
        let _ = writeln!(
            s,
            "repair: BootOrder {} -> {} entries (SetVariable sts={:#x}, verified {})",
            order_n / 2,
            new_n,
            sts,
            ok
        );
    }

    // 4. Delete stale ESP-targeting entries entirely, verify they are gone.
    for e in &entries[..count] {
        if e.covers_esp && !e.file_ok {
            let mut name = [0u16; 32];
            for (j, b) in e.name.iter().enumerate() {
                name[j] = *b as u16;
            }
            let sts = rt.delete_variable(&name, &GLOBAL_GUID);
            let mut scratch = [0u8; 8];
            let gone = read_by_name(rt, &e.name, &mut scratch).is_none();
            let _ = writeln!(
                s,
                "repair: deleted stale {} (SetVariable sts={:#x}, gone {})",
                core::str::from_utf8(&e.name).unwrap_or("?"),
                sts,
                gone
            );
        }
    }
}

/// Create BootNNNN pointing at \EFI\BOOT\BOOTX64.EFI on the mounted ESP.
/// The device path is built from the partition table: HD node with the
/// partition's GPT signature + FilePath + End node. Returns the number.
fn create_esp_entry(s: &mut Serial, rt: &Runtime, vfs: &Vfs, existing: &[BootEntry]) -> Option<u16> {
    let part = &vfs.table.parts[vfs.fat_part];

    // Lowest free Boot#### number.
    let mut num = 0u16;
    loop {
        let mut used = false;
        for e in existing {
            if e.num == num {
                used = true;
            }
        }
        if !used {
            break;
        }
        if num == 0x270F {
            let _ = writeln!(s, "repair: no free Boot#### number");
            return None;
        }
        num += 1;
    }

    // Name: "Boot" + 4 lowercase hex digits.
    const HEX: &[u8] = b"0123456789abcdef";
    let mut name = [0u16; 32];
    name[0] = b'B' as u16;
    name[1] = b'o' as u16;
    name[2] = b'o' as u16;
    name[3] = b't' as u16;
    for i in 0..4 {
        name[4 + i] = HEX[((num >> (12 - 4 * i)) & 0xF) as usize] as u16;
    }

    // Payload: attrs | path_len | description (UTF-16 NUL) | device path.
    let mut data = [0u8; 512];
    data[0..4].copy_from_slice(&LOAD_ACTIVE.to_le_bytes());
    let mut p = 6usize;
    for &c in b"fantuan rescue" {
        data[p] = c;
        data[p + 1] = 0;
        p += 2;
    }
    data[p] = 0;
    data[p + 1] = 0;
    p += 2;

    let path_start = p;
    // HD node (type 4, subtype 1, len 42): partition number, start, size,
    // GPT signature, partition format 0x02, signature type 0x02.
    data[p] = 0x04;
    data[p + 1] = 0x01;
    data[p + 2..p + 4].copy_from_slice(&42u16.to_le_bytes());
    data[p + 4..p + 8].copy_from_slice(&((vfs.fat_part as u32 + 1).to_le_bytes()));
    data[p + 8..p + 16].copy_from_slice(&part.first_lba.to_le_bytes());
    data[p + 16..p + 24].copy_from_slice(&(part.last_lba - part.first_lba + 1).to_le_bytes());
    data[p + 24..p + 40].copy_from_slice(&part.unique_guid);
    data[p + 40] = 0x02;
    data[p + 41] = 0x02;
    p += 42;
    // FilePath node (type 4, subtype 4): "\EFI\BOOT\BOOTX64.EFI".
    let fpath = b"\\EFI\\BOOT\\BOOTX64.EFI";
    let flen = (fpath.len() + 1) * 2; // incl. NUL
    data[p] = 0x04;
    data[p + 1] = 0x04;
    data[p + 2..p + 4].copy_from_slice(&((4 + flen) as u16).to_le_bytes());
    for (i, &c) in fpath.iter().enumerate() {
        data[p + 4 + 2 * i] = c;
    }
    p += 4 + flen;
    // End node.
    data[p] = 0x7F;
    data[p + 1] = 0xFF;
    data[p + 2..p + 4].copy_from_slice(&4u16.to_le_bytes());
    p += 4;
    data[4..6].copy_from_slice(&((p - path_start) as u16).to_le_bytes());

    let sts = rt.set_variable(&name, &GLOBAL_GUID, NV_BS_RT, &data[..p]);
    let mut verify = [0u8; 512];
    let mut vname = [0u8; 8];
    let num_text = boot_num_to_text(num, &mut vname);
    let vn = read_by_name(rt, num_text, &mut verify);
    let ok = sts == 0 && vn.is_some();
    let _ = writeln!(s, "repair: created Boot{:04x} -> ESP fallback (SetVariable sts={:#x}, verified {})", num, sts, ok);
    if ok {
        Some(num)
    } else {
        None
    }
}

fn boot_num_to_text(num: u16, out: &mut [u8; 8]) -> &[u8] {
    const HEX: &[u8] = b"0123456789abcdef";
    out[0..4].copy_from_slice(b"Boot");
    for i in 0..4 {
        out[4 + i] = HEX[((num >> (12 - 4 * i)) & 0xF) as usize];
    }
    out
}
