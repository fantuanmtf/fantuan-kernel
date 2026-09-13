//! CPU diagnostics: brand string, family/model/stepping, feature bits,
//! topology, microcode revision. The precise low-level operations (CPUID
//! leaf selection, RDMSR) are inline assembly; everything else is Rust
//! (DESIGN.md §2). Read-only.

use core::fmt::Write;

use super::Severity;
use crate::serial::Serial;

#[inline]
fn cpuid(leaf: u32, subleaf: u32) -> (u32, u32, u32, u32) {
    let mut a = leaf;
    let mut c = subleaf;
    let b: u32;
    let d: u32;
    unsafe {
        // rbx is reserved for LLVM's own use, so save/restore it around the
        // instruction (the canonical hand-rolled CPUID pattern).
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "mov {b:e}, ebx",
            "pop rbx",
            inout("eax") a,
            inout("ecx") c,
            b = out(reg) b,
            out("edx") d,
            options(preserves_flags),
        );
    }
    (a, b, c, d)
}

/// Read an MSR (e.g. IA32_BIOS_SIGN_ID = 0x8B for the microcode revision).
#[inline]
fn rdmsr(msr: u32) -> u64 {
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

pub fn check(s: &mut Serial) -> Severity {
    // --- M5.5: SMBIOS identity lines (graceful when the anchor is absent) ---
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

    // Brand string from leaves 0x80000002..4 (when the leaf range exists).
    let (max_std, _, _, _) = cpuid(0, 0);
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

    // Family/model/stepping from leaf 1.
    let (v1, _, _, _) = cpuid(1, 0);
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

    // Topology via leaf 0xB (SMT level first, core level second). Emulators
    // (QEMU TCG) may return zeroes; fall back to leaf 1's logical-processor
    // count then.
    let (ebx_core, _, _, _) = cpuid(0xB, 0);
    let (_, _, _, edx_all) = cpuid(0xB, 1);
    let mut cores = ebx_core & 0xFFFF;
    let mut threads = edx_all & 0xFFFF;
    if cores == 0 {
        cores = 1;
        threads = (v1 >> 16) & 0xFF;
    }

    // Microcode revision: IA32_BIOS_SIGN_ID, valid after a CPUID(1).
    let mc = rdmsr(0x8B);
    let rev = (mc >> 32) as u32;

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
    let _ = writeln!(s, "  cpu: microcode rev {:#x}", rev);
    let _ = max_std; // referenced for clarity: maximum standard leaf
    Severity::Ok
}
