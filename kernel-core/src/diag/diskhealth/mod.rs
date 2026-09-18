//! M5.5 disk-health diagnostics: DESIGN.md §7. Read-only surface scan,
//! IDENTIFY decode, ATA SMART attributes and the NVMe SMART/Health log; the
//! dispatcher routes protocol-specific decoding to the right format.

use core::ffi::c_void;


extern "C" {
    fn blk_smart_read_data(dev: *mut c_void, out: *mut c_void) -> i32;
    /// ATA SMART READ LOG / NVMe Get Log Page (M8: NVMe SMART/Health = 0x02).
    fn blk_smart_read_log(dev: *mut c_void, page: u8, buf: *mut c_void, sectors: usize) -> i32;
    #[allow(dead_code)]
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
}

// --- StorageId helpers (IDENTIFY decode) ---

pub struct StorageId {
    pub model: [u8; 40],
    pub model_len: usize,
    pub serial: [u8; 20],
    pub serial_len: usize,
    pub capacity_sectors: u64,
    /// IDENTIFY word 217 == 1 means non-rotating (SSD).
    pub is_ssd: bool,
}

/// Driver-decoded identity (ATA IDENTIFY / NVMe Identify Controller+Namespace
/// are decoded in C; the kernel only formats the result).
pub fn identify_strings(_dev: *mut c_void) -> Option<StorageId> {
    let ident = crate::drv::drive_identity()?;
    let mut sid = StorageId {
        model: [0; 40],
        model_len: 0,
        serial: [0; 20],
        serial_len: 0,
        capacity_sectors: ident.sectors,
        is_ssd: ident.ssd != 0,
    };
    for (i, &b) in ident.model.iter().enumerate() {
        if b == 0 || sid.model_len >= sid.model.len() {
            break;
        }
        sid.model[i] = b;
        sid.model_len += 1;
    }
    for (i, &b) in ident.serial.iter().enumerate() {
        if b == 0 || sid.serial_len >= sid.serial.len() {
            break;
        }
        sid.serial[i] = b;
        sid.serial_len += 1;
    }
    Some(sid)
}

// --- ATA SMART decoder ---

#[derive(Default)]
pub struct AtaSmart {
    pub power_on_hours: u64,
    pub reallocated: u32,
    pub pending: u32,
    pub uncorrectable: u32,
}

pub fn ata_smart(dev: *mut c_void) -> Option<AtaSmart> {
    let mut page = [0u8; 512];
    if unsafe { blk_smart_read_data(dev, page.as_mut_ptr() as *mut c_void) } != 0 {
        return None;
    }
    let mut sm = AtaSmart::default();
    for attr_i in 0..30 {
        let off = 2 + attr_i * 12;
        if off + 12 > 512 {
            break;
        }
        let id = page[off];
        if id == 0 {
            continue;
        }
        // 12-byte attribute entry: id(1) flags(2) value(1) worst(1) raw(6).
        let raw6 = (page[off + 5] as u64)
            | ((page[off + 6] as u64) << 8)
            | ((page[off + 7] as u64) << 16)
            | ((page[off + 8] as u64) << 24)
            | ((page[off + 9] as u64) << 32)
            | ((page[off + 10] as u64) << 40);
        match id {
            0x05 => sm.reallocated = raw6 as u32,
            0x09 => sm.power_on_hours = raw6,
            0xC5 => sm.pending = raw6 as u32,
            0xC6 => sm.uncorrectable = raw6 as u32,
            _ => {}
        }
    }
    Some(sm)
}

// --- NVMe SMART payload (log page 0x02) ---

/// Decoded NVMe SMART/Health fields, formatted by the shared report code.
#[derive(Default)]
pub struct NvmeSmart {
    pub power_on_hours: u64,
    pub data_units_read: u64,
    pub data_units_written: u64,
    pub percentage_used: u8,
    pub media_errors: u32,
}

/// NVMe SMART/Health Information (log page 0x02). Field offsets per NVMe 1.4:
/// percentage used at 5, data units read/written at 32/48 (each unit is
/// 1000 x 512 bytes), power-on hours at 128, media errors at 160.
pub fn nvme_smart(dev: *mut c_void) -> Option<NvmeSmart> {
    let mut page = [0u8; 512];
    if unsafe { blk_smart_read_log(dev, 0x02, page.as_mut_ptr() as *mut c_void, 1) } != 0 {
        return None;
    }
    // The log fields are 128-bit; readers take the low 64 (all counters that
    // matter fit) and treat an all-ones field as "not implemented" — QEMU
    // leaves several that way — reporting zero instead of 2^64.
    let counter = |off: usize| -> u64 {
        let mut lo = 0u64;
        for i in 0..8 {
            lo |= (page[off + i] as u64) << (8 * i);
        }
        if lo == u64::MAX {
            0
        } else {
            lo
        }
    };
    Some(NvmeSmart {
        power_on_hours: counter(128),
        data_units_read: counter(32),
        data_units_written: counter(48),
        percentage_used: page[5],
        media_errors: counter(160).min(u32::MAX as u64) as u32,
    })
}

/// SMART for whatever driver is active: ATA attribute page when the device
/// answers it, otherwise the NVMe health log.
pub fn smart_report(dev: *mut c_void) -> (Option<AtaSmart>, Option<NvmeSmart>) {
    if crate::drv::drive_name() == "nvme" {
        (None, nvme_smart(dev))
    } else {
        (ata_smart(dev), None)
    }
}

// --- Surface scan ---

pub use report::{format_line, surface_scan};

mod report;
