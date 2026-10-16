//! Hardware reporting and the storage stack: SMBIOS/ACPI and stage-1
//! diagnostics, then the C driver layer, the VFS mount and the read-only boot
//! diagnosis. A child of `boot`; see `super` for where this sits in the
//! sequence.
//!
//! Split from `boot/mod.rs` in 2026-10 because that file reached 309 lines on
//! the first attempt and the 300-line ratchet rejected it. The seam is real
//! rather than arithmetic: `boot` above makes the CPU and memory work, this
//! half looks at what is attached to them and is the only part that touches
//! the storage stack.

use core::fmt::Write;

use fantuan_abi::BootInfo;

use crate::serial::Serial;

/// SMBIOS, ACPI and the stage-1 hardware diagnostics. Takes no serial handle:
/// the stage runners log through the kernel-core sink, and ACPI is consumed
/// here by the virtualization report, which is its only reader.
pub fn phase_diagnostics(bi: &BootInfo) {
    // --- M5: diagnostics stage 1 (pure Rust core, DESIGN.md §6) ------------
    // M5.5: SMBIOS tables feed the CPU/GPU diagnostics (CONFIG_SMBIOS, C4);
    // a missing anchor degrades gracefully.
    #[cfg(kconfig_smbios)]
    unsafe {
        crate::smbios::init_from_entry(bi.smbios_table);
        crate::smbios::init(0xF_0000, 0x1_0000);
    }
    // M12-1/M12-7/M12-6: ACPI tables feed the virtualization report
    // (CONFIG_VIRT) and the GPU thermal hook (CONFIG_GRAPHICS).
    #[cfg(any(kconfig_virt, kconfig_graphics))]
    let _acpi = crate::acpi::init(bi.rsdp);
    #[cfg(kconfig_virt)]
    crate::diag::virt::report(_acpi.as_ref());

    let stage1 = [
        crate::diag::Check { name: "cpu", run: crate::diag::cpu::check },
        #[cfg(kconfig_graphics)]
        crate::diag::Check { name: "gpu", run: crate::diag::gpu::check },
        crate::diag::Check { name: "ram", run: crate::diag::ram::check },
    ];
    crate::diag::run_stage("1 hardware", &stage1);
}

/// The C driver layer, the VFS mount, stage-2 diagnostics, the read-only boot
/// diagnosis and the boot-complete beep. Returns the mount for the shell.
pub fn phase_storage(bi: &BootInfo, s: &mut Serial) -> Option<crate::vfs::Vfs> {
    // --- M4.5: C driver layer -----------------------------------------------
    let mut mounted: Option<crate::vfs::Vfs> = None;
    if crate::drivers::init() {
        let _ = writeln!(s, "drivers: C layer + AHCI read-only verified");

        // --- M6: VFS + partition table + FAT32 read-only ------------------
        // Mount first: stage 2 reports per-partition filesystem types from
        // the probe table (M5.5) and the ESP bootloaders it found.
        mounted = crate::vfs::init();
        if mounted.is_some() {
            let _ = writeln!(s, "vfs: ok");
        } else {
            let _ = writeln!(s, "vfs: unavailable (boot continues)");
        }

        // --- M5: diagnostics stage 2 (needs the C storage driver) ----------
        // Rescue/diagnostic reporting; CONFIG_RESCUE_REPAIR (C5).
        #[cfg(kconfig_rescue_repair)]
        crate::diag::run_stage("2 storage", &[crate::diag::Check { name: "storage", run: crate::diag::storage::check }]);

        // --- M7: boot repair v1 (read-only diagnosis; CONFIG_RESCUE_REPAIR) -
        // The boot path is READ-ONLY: repairs run solely from the shell's
        // confirmation-gated `grub-fix repair` (iron rule).
        #[cfg(kconfig_rescue_repair)]
        if let Some(v) = mounted {
            // Shared boot repair logs through the kernel-core Log sink.
            crate::bootrepair::diagnose(&mut kernel_core::log::Log::new(), &v, bi.runtime_services);
        }
    } else {
        let _ = writeln!(s, "drivers: AHCI unavailable (boot continues)");
    }

    // Boot-complete signal: one LONG beep — distinct from the short-beep
    // diagnostic codes (DESIGN.md §6.2).
    crate::pit::beep_long();
    let _ = writeln!(s, "beep: boot ok (1 long)");

    mounted
}
