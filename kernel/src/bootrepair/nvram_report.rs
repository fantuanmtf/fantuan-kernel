//! NVRAM diagnosis report (M7.5, DESIGN.md §9): BootCurrent self-test,
//! Secure Boot / SetupMode, BootOrder + Boot#### with stale-entry detection
//! against the mounted ESP, and the firmware clock — all read-only. The
//! Boot#### model lives in nvram.rs; the M7.6 repair actions consume it from
//! nvram_repair.rs.

use core::fmt::Write;

use crate::runtime::{Runtime, GLOBAL_GUID};
use crate::serial::Serial;
use crate::vfs::Vfs;

use super::nvram::{ascii_to_utf16, collect, BootEntry};

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

    // Boot order + entries (shared model, M7.6 repair reuses it).
    let mut entries = [BootEntry::none(); 8];
    let (count, order, order_n) = collect(rt, vfs, &mut entries);
    let _ = write!(s, "nvram: BootOrder {} entries [", order_n / 2);
    for o in order[..order_n].chunks(2) {
        if o.len() == 2 {
            let _ = write!(s, "{:04x} ", u16::from_le_bytes([o[0], o[1]]));
        }
    }
    let _ = writeln!(s, "]");

    for e in &entries[..count] {
        report_entry(s, e, &order[..order_n]);
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

fn report_entry(s: &mut Serial, e: &BootEntry, order: &[u8]) {
    let active = e.data[0] & 0x1 != 0;
    let mut in_order = false;
    for o in order.chunks(2) {
        if o.len() == 2 && u16::from_le_bytes([o[0], o[1]]) == e.num {
            in_order = true;
        }
    }

    let _ = write!(s, "nvram: {} ({}): ", core::str::from_utf8(&e.name).unwrap_or("?"), core::str::from_utf8(&e.desc[..e.desc_len]).unwrap_or("?"));
    let _ = writeln!(s, "{}{}", if active { "active" } else { "inactive" }, if in_order { " [in BootOrder]" } else { "" });

    if e.covers_esp {
        if e.file_ok {
            let _ = write!(s, "  -> ");
            if e.path_len > 0 {
                let _ = s.write(&e.path[..e.path_len]);
            } else {
                let _ = write!(s, "EFI/BOOT/BOOTX64.EFI (default)");
            }
            let _ = writeln!(s, " present on ESP");
        } else {
            let _ = write!(s, "  -> STALE: ");
            if e.path_len > 0 {
                let _ = s.write(&e.path[..e.path_len]);
            } else {
                let _ = write!(s, "default fallback EFI/BOOT/BOOTX64.EFI");
            }
            let _ = writeln!(s, " missing on ESP — entry will fail");
        }
    } else if e.has_hd_node {
        let _ = writeln!(s, "  -> other disk (GPT signature differs) — not checked");
    } else {
        let _ = writeln!(s, "  -> other disk / firmware entry — not checked");
    }
}
