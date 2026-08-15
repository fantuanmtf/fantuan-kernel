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
    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            let vendor = crate::pci::read32(bus, dev, 0, 0);
            if vendor == 0xFFFF_FFFF {
                continue;
            }
            let c = crate::pci::read32(bus, dev, 0, 8);
            if ((c >> 24) as u8) == 0x03 {
                let vendor_id = vendor & 0xFFFF;
                let device_id = vendor >> 16;
                count += 1;
                if vendor_id == 0x8086 {
                    any_intel = true;
                }
                let _ = writeln!(
                    s,
                    "  gpu: {:02x}:{:02x}.0 vendor {:#06x} device {:#06x}",
                    bus, dev, vendor_id, device_id
                );
            }
        }
    }
    GpuInfo { count, any_intel }
}

pub fn check(s: &mut Serial) -> Severity {
    let info = scan(s);
    if info.count == 0 {
        let _ = writeln!(s, "  gpu: none detected — no display device");
        crate::pit::beep_n(4, crate::pit::BeepLen::Short);
        Severity::Critical
    } else if !info.any_intel {
        let _ = writeln!(s, "  gpu: display path present (no Intel iGPU)");
        crate::pit::beep_n(1, crate::pit::BeepLen::Short);
        Severity::Ok
    } else {
        let _ = writeln!(s, "  gpu: display path present");
        Severity::Ok
    }
}
