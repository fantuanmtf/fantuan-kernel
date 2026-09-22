//! x86 rescue/diagnostic commands (CONFIG_RESCUE_REPAIR): hardware
//! diagnostics, PCI/drive listing and the crypto self-test. The shared
//! rescue commands (diskhealth, lsos, cat, ...) live in kernel-core; the
//! module is not compiled in the minimal kernel.

use core::fmt::Write;

use crate::diag;
use crate::drivers;
use kernel_core::log::Log;
use kernel_core::shell::Shell;

macro_rules! out {
    ($s:expr, $($arg:tt)*) => {
        { let _ = writeln!($s, $($arg)*); }
    };
}

pub fn cmd_hwdiag(_sh: &mut Shell, _s: &mut Log, _args: &[&[u8]]) {
    let stage1 = [
        diag::Check { name: "cpu", run: diag::cpu::check },
        #[cfg(kconfig_graphics)]
        diag::Check { name: "gpu", run: diag::gpu::check },
        diag::Check { name: "ram", run: diag::ram::check },
    ];
    diag::run_stage("1 hardware", &stage1);
    let stage2: [diag::Check; 1] = [
        diag::Check { name: "storage", run: diag::storage::check },
    ];
    diag::run_stage("2 storage", &stage2);
}

pub fn cmd_lsdev(_sh: &mut Shell, s: &mut Log, _args: &[&[u8]]) {
    for d in crate::pci::find_storage_controllers() {
        out!(s, "  storage {:02x}:{:02x}.{} vendor {:#06x} device {:#06x} class {:02x}/{:02x} progif {:#04x}",
            d.bus, d.dev, d.func, d.vendor, d.device, d.class, d.subclass, d.progif);
    }
    for d in crate::pci::list_display_devices() {
        out!(s, "  display {:02x}:{:02x}.{} vendor {:#06x} device {:#06x}",
            d.bus, d.dev, d.func, d.vendor, d.device);
    }
    match diag::diskhealth::identify_strings(drivers::drive_handle()) {
        Some(id) => out!(
            s,
            "  drive: {} (sn {}) — {} sectors, {} MiB, {}",
            core::str::from_utf8(&id.model[..id.model_len]).unwrap_or("?"),
            core::str::from_utf8(&id.serial[..id.serial_len]).unwrap_or("?"),
            id.capacity_sectors,
            id.capacity_sectors / 2048,
            if id.is_ssd { "SSD" } else { "HDD" }
        ),
        None => out!(s, "  drive: IDENTIFY unavailable"),
    }
}

pub fn cmd_crypto(_sh: &mut Shell, s: &mut Log, _args: &[&[u8]]) {
    if crate::crypto::selftest(s, true) {
        out!(s, "crypto: all known-answer tests passed");
    } else {
        out!(s, "crypto: FAILURES above — do not trust authenticated bundles");
    }
}

/// M12-6: re-print the GPU/PCI report (the same block the boot stage prints).
#[cfg(kconfig_graphics)]
pub fn cmd_gpu(_sh: &mut Shell, s: &mut Log, _args: &[&[u8]]) {
    diag::gpu::report(s);
}
