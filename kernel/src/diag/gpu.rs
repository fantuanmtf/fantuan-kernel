//! GPU presence check (DESIGN.md §6.1): PCI enumeration only — no rendering
//! tests, no GPU driver (the console is the GOP framebuffer). Drives the
//! §6.2 beep codes:
//!   4 short  — no display device at all (critical)
//!   1 short  — no Intel-classified iGPU (informational)
//! The "2 short" code (dGPU plugged in but missing) needs SMBIOS slot
//! status. The module is CONFIG_GRAPHICS-gated (C4); its SMBIOS input is
//! additionally CONFIG_SMBIOS-gated and degrades to "no slots".

use core::fmt::Write;

use super::Severity;
use kernel_core::log::Log;

pub struct GpuInfo {
    pub count: u32,
    pub any_intel: bool,
}

pub fn scan(s: &mut Log) -> GpuInfo {
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

/// SMBIOS slot facts: (total slots, PCI in-use, graphics-capable in-use).
#[cfg(kconfig_smbios)]
fn slot_facts() -> (usize, usize, bool) {
    let slots = crate::smbios::system_slots();
    (
        slots.len(),
        slots.iter().filter(|s| s.in_use && s.uses_pci).count(),
        slots.iter().any(|s| s.in_use && s.display_class_hint),
    )
}

#[cfg(not(kconfig_smbios))]
fn slot_facts() -> (usize, usize, bool) {
    (0, 0, false)
}

#[cfg(kconfig_smbios)]
fn print_in_use_slots(s: &mut Log) {
    for sl in crate::smbios::system_slots()
        .iter()
        .filter(|s| s.in_use && s.display_class_hint)
    {
        let _ = writeln!(s, "  gpu: slot {} '{}' In Use", sl.slot_id, sl.designation);
    }
}

#[cfg(not(kconfig_smbios))]
fn print_in_use_slots(_s: &mut Log) {}

pub fn check(s: &mut Log) -> Severity {
    let info = scan(s);
    let (slots_total, pci_in_use, slot_in_use) = slot_facts();
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
    if slots_total > 0 {
        let _ = writeln!(
            s,
            "  gpu: smbios slots {} (pci in-use {}), display devices {}",
            slots_total,
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
    if slot_in_use && !has_dgpu {
        print_in_use_slots(s);
        let _ = writeln!(s, "  gpu: slot marked In Use but no discrete GPU enumerated (2-short beep)");
        crate::pit::beep_n(2, crate::pit::BeepLen::Short);
        worst = Severity::Warning;
    }
    worst
}
