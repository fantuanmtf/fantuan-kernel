//! Stage-2 storage diagnostics (DESIGN.md §8.1/§7): boot-header scan (MBR /
//! GPT) and IDENTIFY through the C AHCI driver. Read-only — nothing here ever
//! writes to the disk.

use core::ffi::c_void;
use core::fmt::Write;

use super::Severity;
use crate::serial::Serial;

extern "C" {
    fn ata_identify(dst: *mut c_void) -> i32;
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
}

/// ATA words in memory are byte-swapped: word N lives at bytes [2N+1, 2N].
fn ata_word(id: &[u8; 512], n: usize) -> u16 {
    u16::from_le_bytes([id[n * 2], id[n * 2 + 1]])
}

pub fn check(s: &mut Serial) -> Severity {
    // Boot-header scan: LBA0 carries either the MBR signature (0x55AA at
    // offset 510) or a GPT header ("EFI PART").
    let mut lba0 = [0u8; 512];
    if unsafe { blk_read(core::ptr::null_mut(), 0, lba0.as_mut_ptr() as *mut c_void, 1) } != 0 {
        let _ = writeln!(s, "  storage: LBA0 read failed");
        return Severity::Critical;
    }
    let is_mbr = lba0[510] == 0x55 && lba0[511] == 0xAA;
    let is_gpt = &lba0[..8] == b"EFI PART";

    // IDENTIFY DEVICE: model, serial, LBA48 capacity.
    let mut id = [0u8; 512];
    if unsafe { ata_identify(id.as_mut_ptr() as *mut c_void) } != 0 {
        let _ = writeln!(s, "  storage: IDENTIFY failed");
        return Severity::Warning;
    }
    let mut model = [0u8; 40];
    for i in 0..20 {
        model[i * 2] = id[54 + i * 2 + 1];
        model[i * 2 + 1] = id[54 + i * 2];
    }
    let mut mlen = 40;
    while mlen > 0 && model[mlen - 1] == b' ' {
        mlen -= 1;
    }
    let mut serial = [0u8; 20];
    for i in 0..10 {
        serial[i * 2] = id[20 + i * 2 + 1];
        serial[i * 2 + 1] = id[20 + i * 2];
    }
    let mut slen = 20;
    while slen > 0 && serial[slen - 1] == b' ' {
        slen -= 1;
    }
    let mut capacity: u64 = 0;
    for i in 0..4 {
        capacity |= (ata_word(&id, 100 + i) as u64) << (16 * i);
    }

    let _ = writeln!(
        s,
        "  storage: {} (sn {}) — {} sectors, {} MiB",
        core::str::from_utf8(&model[..mlen]).unwrap_or("?"),
        core::str::from_utf8(&serial[..slen]).unwrap_or("?"),
        capacity,
        capacity / 2048
    );
    let _ = writeln!(s, "  storage: LBA0 boot header: mbr {} gpt {}", is_mbr, is_gpt);
    Severity::Ok
}
