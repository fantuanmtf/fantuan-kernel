//! M5.5 disk-health diagnostics: DESIGN.md §7, Task 4. Read-only surface scan,
//! IDENTIFY decode, and ATA SMART attributes. NVMe SMART is a stub until Task 8;
//! the dispatcher already routes protocol checks so later work is surgical.

use core::ffi::c_void;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};

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
    let ident = crate::drivers::drive_identity()?;
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

// --- NVMe SMART dispatcher (stub until Task 8) ---

/// NVMe SMART payload — the dispatcher stub keeps the formatting code shared
/// until the NVMe driver lands (Task 8).
#[allow(dead_code)]
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
    if crate::drivers::drive_name() == "nvme" {
        (None, nvme_smart(dev))
    } else {
        (ata_smart(dev), None)
    }
}

// --- Surface scan ---

/// Surface scan — opt-in from the shell (DESIGN.md §7 "off by default"),
/// capped at 4 GiB per the rescue iron rule.
#[allow(dead_code)]
pub struct ScanResult {
    pub total_sectors: u64,
    pub slow_sectors: u64,
    pub read_errors: u64,
}

#[allow(dead_code)]
const DEFAULT_CAP_GIB: u64 = 4;

#[allow(dead_code)]
pub fn surface_scan<F: FnMut(u64, u64)>(
    dev: *mut c_void,
    start_lba: u64,
    count_sectors: u64,
    mut on_progress: F,
    cancel: &AtomicBool,
) -> ScanResult {
    let cap_sectors = (DEFAULT_CAP_GIB << 30) / 512;
    let mut sectors = count_sectors;
    if sectors > cap_sectors {
        sectors = cap_sectors;
    }
    let mut r = ScanResult { total_sectors: 0, slow_sectors: 0, read_errors: 0 };
    let mut buf = [0u8; 512];
    let start_ts = crate::tsc::now();
    let mut last_progress = 0u64;
    let mut i = 0u64;
    while i < sectors {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let t0 = crate::tsc::now();
        let ok = unsafe {
            blk_read(dev, start_lba + i, buf.as_mut_ptr() as *mut c_void, 1)
        } == 0;
        let t1 = crate::tsc::now();
        r.total_sectors += 1;
        if !ok {
            r.read_errors += 1;
        }
        let ns = crate::tsc::to_nanos(t1.saturating_sub(t0));
        if ns > 500_000_000u128 {
            r.slow_sectors += 1;
        }
        let pct = ((i + 1) * 100) / sectors.max(1);
        if pct != last_progress {
            on_progress(i + 1, sectors);
            last_progress = pct;
        }
        i += 1;
    }
    let _ = start_ts;
    r
}

// --- Public formatting API ---

fn write_u64(s: &mut dyn Write, mut v: u64) {
    let mut buf = [0u8; 20];
    let mut i = 0;
    if v == 0 {
        let _ = s.write_char('0');
        return;
    }
    while v > 0 {
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        i += 1;
    }
    for j in (0..i).rev() {
        let _ = s.write_char(buf[j] as char);
    }
}

fn write_u32(s: &mut dyn Write, v: u32) {
    write_u64(s, v as u64);
}

fn write_u8(s: &mut dyn Write, v: u8) {
    write_u64(s, v as u64);
}

fn write_tb_or_gb(s: &mut dyn Write, gib: u64) {
    if gib >= 1024 {
        let tb = gib / 1024;
        write_u64(s, tb);
        let _ = s.write_str(".");
        write_u64(s, (gib % 1024) * 10 / 1024);
        let _ = s.write_str(" TB");
    } else {
        write_u64(s, gib);
        let _ = s.write_str(" GB");
    }
}

pub fn format_line(
    s: &mut dyn Write,
    id: &StorageId,
    ata: Option<&AtaSmart>,
    nvme: Option<&NvmeSmart>,
) {
    let ml = id.model_len.max(1);
    let model_bytes = &id.model[..ml];
    for b in model_bytes {
        let _ = s.write_char(*b as char);
    }
    let pad = 8usize.saturating_sub(id.model_len);
    for _ in 0..pad {
        let _ = s.write_char(' ');
    }
    let is_nvme = nvme.is_some();
    let kind = if is_nvme || id.is_ssd { "SSD" } else { "HDD" };
    let _ = s.write_str("  ");
    let _ = s.write_str(kind);
    let _ = s.write_str("   power-on ");
    let poh = if is_nvme {
        nvme.map(|n| n.power_on_hours).unwrap_or(0)
    } else {
        ata.map(|a| a.power_on_hours).unwrap_or(0)
    };
    write_u64(s, poh);
    let _ = s.write_str(" h");
    if is_nvme {
        if let Some(n) = nvme {
            // One data unit = 1000 x 512 bytes (NVMe spec).
            const BYTES_PER_UNIT: u128 = 1000 * 512;
            const GIB: u128 = 1024 * 1024 * 1024;
            let gib_r = ((n.data_units_read as u128) * BYTES_PER_UNIT / GIB) as u64;
            let gib_w = ((n.data_units_written as u128) * BYTES_PER_UNIT / GIB) as u64;
            let _ = s.write_str(" (");
            write_u64(s, poh / 24);
            let _ = s.write_str(" d)   read ");
            write_tb_or_gb(s, gib_r);
            let _ = s.write_str("   written ");
            write_tb_or_gb(s, gib_w);
            let _ = s.write_str("   life ");
            write_u8(s, n.percentage_used);
            let _ = s.write_char('%');
        }
    } else if let Some(a) = ata {
        let _ = s.write_str("   reallocated ");
        write_u32(s, a.reallocated);
        let _ = s.write_str("   pending ");
        write_u32(s, a.pending);
        let _ = s.write_str("   uncorrectable ");
        write_u32(s, a.uncorrectable);
        let _ = s.write_str("   [not scanned]");
    }
    let _ = s.write_char('\n');
}
