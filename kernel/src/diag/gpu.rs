//! GPU presence check (DESIGN.md §6.1): PCI enumeration only — no rendering
//! tests, no GPU driver (the console is the GOP framebuffer). Drives the
//! §6.2 beep codes:
//!   4 short  — no display device at all (critical)
//!   1 short  — no Intel-classified iGPU (informational)
//! The "2 short" code (dGPU plugged in but missing) needs SMBIOS slot
//! status; deferred to M5.5 with the SMBIOS parser.

use core::fmt::Write;

use super::Severity;
use crate::serial::Serial;

pub struct GpuInfo {
    pub count: u32,
    pub any_intel: bool,
    /// A Type-9 slot is In Use and graphics-capable (§6.2 "2 short" input).
    pub slot_in_use: bool,
}

pub fn scan(s: &mut Serial) -> GpuInfo {
    let mut count = 0;
    let mut any_intel = false;
    // M5.5: the PCI catalog owns enumeration; this check only classifies.
    for d in crate::pci::list_display_devices() {
        count += 1;
        if d.vendor == 0x8086 {
            any_intel = true;
        }
        let _ = writeln!(
            s,
            "  gpu: {:02x}:{:02x}.{} vendor {:#06x} device {:#06x}",
            d.bus, d.dev, d.func, d.vendor, d.device
        );
    }
    let _ = writeln!(s, "  gpu: pci display devices found: {}", count);
    let slot_in_use = crate::smbios::system_slots()
        .iter()
        .any(|sl| sl.in_use && sl.display_class_hint);
    GpuInfo { count, any_intel, slot_in_use }
}

pub fn check(s: &mut Serial) -> Severity {
    let info = scan(s);
    let displays = crate::pci::list_display_devices();
    let has_dgpu = displays.iter().any(|d| d.vendor != 0x8086);
    let has_igpu = info.any_intel;

    // §6.2 beep codes. 4 short supersedes 2+1 — when nothing is detected we
    // beep 4 and never emit an ambiguous 2+1 sequence.
    if info.count == 0 {
        let _ = writeln!(s, "  gpu: none detected — no display device");
        crate::pit::beep_n(4, crate::pit::BeepLen::Short);
        return Severity::Critical;
    }

    // Informational: how the SMBIOS slot table lines up with the PCI catalog.
    let slots = crate::smbios::system_slots();
    if !slots.is_empty() {
        let pci_in_use = slots.iter().filter(|s| s.in_use && s.uses_pci).count();
        let _ = writeln!(
            s,
            "  gpu: smbios slots {} (pci in-use {}), display devices {}",
            slots.len(),
            pci_in_use,
            info.count
        );
    }

    let mut worst = Severity::Ok;
    if !has_igpu {
        let _ = writeln!(s, "  gpu: display path present (no Intel iGPU)");
        crate::pit::beep_n(1, crate::pit::BeepLen::Short);
    } else {
        let _ = writeln!(s, "  gpu: display path present");
    }

    // 2 short: a Type-9 slot is In Use (a card is seated in a graphics-capable
    // slot) but no discrete GPU answered on PCI.
    if info.slot_in_use && !has_dgpu {
        for sl in slots.iter().filter(|s| s.in_use && s.display_class_hint) {
            let _ = writeln!(s, "  gpu: slot {} '{}' In Use", sl.slot_id, sl.designation);
        }
        let _ = writeln!(s, "  gpu: slot marked In Use but no discrete GPU enumerated (2-short beep)");
        crate::pit::beep_n(2, crate::pit::BeepLen::Short);
        worst = Severity::Warning;
    }
    worst
}

