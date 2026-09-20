//! CPU diagnostics: brand string, family/model/stepping, feature bits,
//! topology, microcode revision. The precise low-level operations (CPUID
//! leaf selection, RDMSR) are inline assembly; everything else is Rust
//! (DESIGN.md §2). Read-only.

use core::fmt::Write;

use super::Severity;
use kernel_core::log::Log;

use crate::cpu::cpuid;

/// Read an MSR (e.g. IA32_BIOS_SIGN_ID = 0x8B for the microcode revision).
#[inline]
pub(crate) fn rdmsr(msr: u32) -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(nomem, preserves_flags),
        );
    }
    ((hi as u64) << 32) | lo as u64
}

pub fn check(s: &mut Log) -> Severity {
    // --- M5.5: SMBIOS identity lines (CONFIG_SMBIOS; C4) -------------------
    #[cfg(kconfig_smbios)]
    {
        match crate::smbios::bios_info() {
            Some(b) => {
                if b.release_date.is_empty() {
                    let _ = writeln!(s, "  smbios: BIOS {} {}", b.vendor, b.version);
                } else {
                    let _ = writeln!(s, "  smbios: BIOS {} {} ({})", b.vendor, b.version, b.release_date);
                }
            }
            None => {
                let _ = writeln!(s, "  smbios: unavailable (no entry point found)");
            }
        }
        if crate::smbios::corrupt() {
            let _ = writeln!(s, "  smbios: abort (corrupt structure table)");
        }
        if let Some(sys) = crate::smbios::system_info() {
            let _ = writeln!(
                s,
                "  smbios: System {} {} (sn {})",
                sys.manufacturer, sys.product_name, sys.serial_number
            );
        }
        let dimms = crate::smbios::memory_devices();
        if !dimms.is_empty() {
            let mut total_mb: u64 = 0;
            for d in dimms {
                total_mb += d.size_mb as u64;
            }
            let _ = write!(s, "  smbios: DIMMs {} x {} MiB", dimms.len(), total_mb);
            if dimms[0].speed_mtps > 0 {
                let _ = write!(s, " @ {} MT/s", dimms[0].speed_mtps);
            }
            if !dimms[0].manufacturer.is_empty() || !dimms[0].part_number.is_empty() {
                let _ = write!(s, " [{} {}]", dimms[0].manufacturer, dimms[0].part_number);
            }
            let _ = writeln!(s);
        }
    }

    // Brand string from leaves 0x80000002..4 (when the leaf range exists).
    let (max_std, v0b, v0c, v0d) = cpuid(0, 0);
    let intel = v0b == 0x756e_6547 && v0d == 0x4965_6e69 && v0c == 0x6c65_746e;
    let (max_ext, _, _, _) = cpuid(0x8000_0000, 0);
    let mut brand = [0u8; 48];
    let mut blen = 0;
    if max_ext >= 0x8000_0004 {
        for (i, leaf) in [0x8000_0002u32, 0x8000_0003, 0x8000_0004].iter().enumerate() {
            let (a, b, c, d) = cpuid(*leaf, 0);
            for (j, w) in [a, b, c, d].iter().enumerate() {
                let off = i * 16 + j * 4;
                brand[off..off + 4].copy_from_slice(&w.to_le_bytes());
            }
        }
        blen = 48;
        while blen > 0 && (brand[blen - 1] == 0 || brand[blen - 1] == b' ') {
            blen -= 1;
        }
    }

    // Family/model/stepping from leaf 1 (EDX bit 28 = HTT).
    let (v1, _, _, d1) = cpuid(1, 0);
    let family = ((v1 >> 8) & 0xF) + ((v1 >> 20) & 0xFF);
    let model = ((v1 >> 4) & 0xF) + (((v1 >> 16) & 0xF) << 4);
    let stepping = v1 & 0xF;

    // Security-relevant feature bits: NX (ext leaf 1, EDX bit 20),
    // SMEP/SMAP (leaf 7, EBX bits 7/20).
    let (_, _, _, d_ext) = cpuid(0x8000_0001, 0);
    let (_, b7, _, _) = cpuid(7, 0);
    let nx = d_ext & (1 << 20) != 0;
    let smep = b7 & (1 << 7) != 0;
    let smap = b7 & (1 << 20) != 0;

    // Topology via leaf 0xB: ECX bits 15:8 hold the level type (1 = SMT,
    // 2 = core) and EBX the logical-processor count at that level. The core
    // count is the core-level EBX divided by the SMT-level EBX. Emulators may
    // return zeroes; fall back to leaf 1's logical count then.
    let (_, ebx0, ecx0, _) = cpuid(0xB, 0);
    let (_, ebx1, ecx1, _) = cpuid(0xB, 1);
    let type0 = (ecx0 >> 8) & 0xFF;
    let type1 = (ecx1 >> 8) & 0xFF;
    let smt_ebx = if type0 == 1 { ebx0 & 0xFFFF } else if type1 == 1 { ebx1 & 0xFFFF } else { 0 };
    let core_ebx = if type0 == 2 { ebx0 & 0xFFFF } else if type1 == 2 { ebx1 & 0xFFFF } else { 0 };
    let logical = ((v1 >> 16) & 0xFF).max(1);
    let (cores, threads) = if smt_ebx > 0 && core_ebx > 0 {
        (core_ebx / smt_ebx, smt_ebx)
    } else if d1 & (1 << 28) != 0 {
        (logical / 2, logical) // HTT set: logical count includes siblings
    } else {
        (logical, logical)
    };
    let cores = cores.max(1);


    let _ = writeln!(
        s,
        "  cpu: {} family {} model {} stepping {}",
        core::str::from_utf8(&brand[..blen]).unwrap_or("unknown"),
        family,
        model,
        stepping
    );
    let _ = writeln!(
        s,
        "  cpu: {} cores / {} threads, nx {} smep {} smap {}",
        cores, threads, nx, smep, smap
    );
    // Microcode revision: IA32_BIOS_SIGN_ID, valid after a CPUID(1). The
    // MSR is Intel-only and may #GP elsewhere, so gate it on the vendor.
    if intel {
        let mc = rdmsr(0x8B);
        let _ = writeln!(s, "  cpu: microcode rev {:#x}", (mc >> 32) as u32);
    } else {
        let _ = writeln!(s, "  cpu: microcode n/a (non-Intel vendor)");
    }
    let _ = max_std; // referenced for clarity: maximum standard leaf
    Severity::Ok
}
