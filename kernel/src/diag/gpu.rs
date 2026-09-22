//! GPU/PCI report (M12-6, report-only): every display-class device (class
//! 0x03) with the vendor/device/subsystem IDs and a known-id name, every BAR
//! sized by the standard write-1s configuration probe (config space only,
//! restored immediately), the aperture mapped through the PHYS_OFFSET window
//! and read once (no writes, no register pokes), the PCIe current/max link
//! speed and width when the device is PCIe, and ACPI thermal-zone
//! availability (`_TZ_` in the DSDT; temperatures are not evaluated).
//!
//! The same block is printed by the boot stage and re-printed by the shell
//! `gpu` command. The module stays CONFIG_GRAPHICS-gated (C4) so the minimal
//! kernel links none of it; the SMBIOS slot input degrades to "no slots".
//! Design: docs/M12_TOOLS_HW.md §4.

use core::fmt::Write;

use super::Severity;
use kernel_core::log::Log;

use crate::arch::x86_64::pci_probe;
use crate::pci::PciDevice;

/// Never map more than this much MMIO for the reachability read (bounded:
/// 128 x 2 MiB page-table entries at most). Larger VRAM BARs are reported
/// but left unmapped.
const MAP_CAP: u64 = 256 * 1024 * 1024;

pub struct GpuInfo {
    pub count: u32,
    pub any_intel: bool,
}

fn vendor_name(vendor: u16) -> Option<&'static str> {
    match vendor {
        0x1002 => Some("AMD/ATI"),
        0x1013 => Some("Cirrus Logic"),
        0x1234 => Some("QEMU/Bochs"),
        0x10DE => Some("NVIDIA"),
        0x8086 => Some("Intel"),
        0x1AF4 => Some("Red Hat virtio"),
        0x1B36 => Some("Red Hat QXL"),
        _ => None,
    }
}

/// Known display-device IDs. The AMD rows cover the RX 500/5000/6000
/// families named in M12_TOOLS_HW §7 plus common iGPUs; an unknown ID still
/// reports the raw vendor:device and the vendor name.
fn device_name(vendor: u16, device: u16) -> Option<&'static str> {
    match (vendor, device) {
        (0x1234, 0x1111) => Some("QEMU stdvga"),
        (0x1013, 0x00B8) => Some("Cirrus GD5446"),
        (0x1AF4, 0x1050) => Some("virtio-gpu"),
        (0x1B36, 0x0100) => Some("QXL paravirtual"),
        (0x1002, 0x67DF) => Some("Radeon RX 470/480/570/580 (Polaris 10)"),
        (0x1002, 0x67EF) => Some("Radeon RX 460/560 (Polaris 11)"),
        (0x1002, 0x67FF) => Some("Radeon RX 550/640 (Polaris 12)"),
        (0x1002, 0x687F) => Some("Radeon RX Vega 56/64 (Vega 10)"),
        (0x1002, 0x731F) => Some("Radeon RX 5700/5700 XT (Navi 10)"),
        (0x1002, 0x7340) => Some("Radeon RX 5500 XT (Navi 14)"),
        (0x1002, 0x73BF) => Some("Radeon RX 6800/6900 XT (Navi 21)"),
        (0x1002, 0x73DF) => Some("Radeon RX 6700 XT (Navi 22)"),
        (0x1002, 0x73FF) => Some("Radeon RX 6600/6600 XT (Navi 23)"),
        (0x1002, 0x7422) => Some("Radeon RX 6500 XT (Navi 24)"),
        (0x1002, 0x743F) => Some("Radeon RX 6400 (Navi 24)"),
        (0x1002, 0x15DD) => Some("Radeon Vega iGPU (Raven)"),
        (0x1002, 0x1636) => Some("Radeon iGPU (Renoir)"),
        _ => None,
    }
}

fn fmt_size(size: u64) -> (u64, &'static str) {
    if size >= 1 << 30 && size % (1 << 30) == 0 {
        (size >> 30, "G")
    } else if size >= 1 << 20 && size % (1 << 20) == 0 {
        (size >> 20, "M")
    } else if size >= 1 << 10 && size % (1 << 10) == 0 {
        (size >> 10, "K")
    } else {
        (size, "B")
    }
}

/// Map the BAR through the PHYS_OFFSET window and read its first dword once
/// (identity/idle register or framebuffer start). Interrupts are held off
/// across the page-table update; nothing is written to the device.
fn map_and_probe(base: u64, size: u64) -> bool {
    if size == 0 || size > MAP_CAP || base.checked_add(size).is_none() {
        return false;
    }
    let flags = kernel_core::arch::irq_save();
    let virt = crate::mm::paging::map_mmio(crate::mm::frame::get(), base, size);
    if let Some(v) = virt {
        unsafe {
            core::ptr::read_volatile(v as *const u32);
        }
    }
    kernel_core::arch::irq_restore(flags);
    virt.is_some()
}

fn report_bars(s: &mut Log, d: &PciDevice) {
    let mut skip_next = false;
    for i in 0..6u8 {
        if skip_next {
            skip_next = false;
            continue;
        }
        let b = match pci_probe::bar(d.bus, d.dev, d.func, i) {
            Some(b) => b,
            None => continue,
        };
        if b.is_64 {
            skip_next = true;
        }
        let (n, unit) = fmt_size(b.size);
        if b.is_io {
            let _ = writeln!(s, "  gpu: bar{} {:#x} size {}{} (io)", b.index, b.base, n, unit);
            continue;
        }
        let mapped = map_and_probe(b.base, b.size);
        let kind = if mapped { "mapped ro" } else { "not mapped" };
        let width = if b.is_64 { ", 64-bit" } else { "" };
        let _ = writeln!(
            s,
            "  gpu: bar{} {:#x} size {}{} ({}{})",
            b.index, b.base, n, unit, kind, width
        );
    }
}

fn report_device(s: &mut Log, d: &PciDevice) {
    let (sv, sd) = pci_probe::subsystem_ids(d.bus, d.dev, d.func);
    let name = device_name(d.vendor, d.device)
        .or_else(|| vendor_name(d.vendor))
        .unwrap_or("unknown");
    let _ = write!(
        s,
        "  gpu: {:02x}:{:02x}.{} {:04x}:{:04x} {} [display]",
        d.bus, d.dev, d.func, d.vendor, d.device, name
    );
    if sv != 0 || sd != 0 {
        let _ = write!(s, " ss={:04x}:{:04x}", sv, sd);
    }
    let _ = writeln!(s);
    report_bars(s, d);
    match pci_probe::pcie_link(d.bus, d.dev, d.func) {
        Some(l) => {
            let _ = write!(s, "  gpu: pcie link {} x{}", pci_probe::speed_name(l.speed), l.width);
            if l.max_speed != l.speed || l.max_width != l.width {
                let _ = write!(s, " (max {} x{})", pci_probe::speed_name(l.max_speed), l.max_width);
            }
            let _ = writeln!(s);
        }
        None => {
            let _ = writeln!(s, "  gpu: pcie n/a (no PCIe capability)");
        }
    }
}

/// The stable `gpu:` block: one device block per display controller, the
/// device count, and the ACPI thermal hook (once).
pub fn report(s: &mut Log) -> GpuInfo {
    let mut count = 0;
    let mut any_intel = false;
    for d in crate::pci::list_display_devices() {
        count += 1;
        if d.vendor == 0x8086 {
            any_intel = true;
        }
        report_device(s, d);
    }
    let _ = writeln!(s, "  gpu: pci display devices found: {}", count);
    match crate::acpi::thermal_zone() {
        Some(true) => {
            let _ = writeln!(
                s,
                "  gpu: thermal zone present (ACPI _TZ_; temperature read not implemented)"
            );
        }
        Some(false) => {
            let _ = writeln!(s, "  gpu: thermal unavailable (no ACPI TZ)");
        }
        None => {
            let _ = writeln!(s, "  gpu: thermal unavailable (no ACPI table walk)");
        }
    }
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
    let info = report(s);
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
