//! Disk-health reporting + the opt-in surface scan (DESIGN.md §7) — split
//! out of mod.rs to keep every file inside the size rule.

use core::ffi::c_void;
use core::fmt::Write;
use core::sync::atomic::{AtomicBool, Ordering};

use super::{AtaSmart, NvmeSmart, StorageId};

extern "C" {
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
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
