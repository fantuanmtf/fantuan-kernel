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
    GpuInfo { count, any_intel }
}

pub fn check(s: &mut Serial) -> Severity {
    let info = scan(s);
    let sev = check_slots_vs_pci(s);
    let worst = if sev > info.severity() { sev } else { info.severity() };
    if info.count == 0 {
        let _ = writeln!(s, "  gpu: none detected — no display device");
        crate::pit::beep_n(4, crate::pit::BeepLen::Short);
        Severity::Critical
    } else if !info.any_intel {
        let _ = writeln!(s, "  gpu: display path present (no Intel iGPU)");
        crate::pit::beep_n(1, crate::pit::BeepLen::Short);
        worst
    } else {
        let _ = writeln!(s, "  gpu: display path present");
        worst
    }
}

impl GpuInfo {
    fn severity(&self) -> Severity {
        if self.count == 0 { Severity::Critical } else { Severity::Ok }
    }
}

/// Task-2-owned GPU slot ↔ PCI display-device correlation.
/// Returns Warning and fires 2-short beep once if any Type-9 slot marked In Use and
/// display-class-hint matched has no corresponding PCI class-0x03 device.
pub fn check_slots_vs_pci(s: &mut Serial) -> Severity {
    let slots = crate::smbios::system_slots();
    let displays = crate::pci::list_display_devices();
    let mut pci_slots_in_use = 0usize;
    let mut mismatch = 0;
    for slot in slots.iter() {
        if slot.in_use && slot.uses_pci {
            pci_slots_in_use += 1;
        }
        if slot.in_use && slot.display_class_hint {
            let found = !displays.is_empty();
            if !found {
                mismatch += 1;
                let _ = writeln!(s, "  gpu: slot {} '{}' marked in-use (display-class) but no PCI 0x03 device",
                    slot.slot_id, slot.designation);
            }
        }
    }
    if !slots.is_empty() {
        let _ = writeln!(
            s,
            "  gpu: smbios slots {} (pci in-use {}), pci display devices {}",
            slots.len(),
            pci_slots_in_use,
            displays.len()
        );
    }
    if mismatch > 0 {
        crate::pit::beep_n(2, crate::pit::BeepLen::Short);
        let _ = writeln!(s, "  gpu: {} display slot(s) in-use but no PCI class 0x03 (2-short beep)", mismatch);
        Severity::Warning
    } else {
        Severity::Ok
    }
}
