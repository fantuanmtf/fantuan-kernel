//! Virtualization detection v1 (M12-7): what the CPU and firmware offer,
//! printed as a short verdict plus the compatibility facts M14/M15 need.
//! Read-only: CPUID and the ACPI tables only; no MSR is touched unless the
//! CPUID feature that guarantees it is present.

use core::fmt::Write;

use crate::acpi::Tables;
use crate::cpu::cpuid;
use crate::diag::cpu::rdmsr;
use crate::serial::{self, Serial};

const MSR_FEATURE_CONTROL: u32 = 0x3A;
const MSR_EPT_VPID_CAP: u32 = 0x48C;

/// 12-byte hypervisor vendor from leaf 0x40000000.
fn hypervisor_vendor() -> [u8; 12] {
    let (_, b, c, d) = cpuid(0x4000_0000, 0);
    let mut v = [0u8; 12];
    v[0..4].copy_from_slice(&b.to_le_bytes());
    v[4..8].copy_from_slice(&c.to_le_bytes());
    v[8..12].copy_from_slice(&d.to_le_bytes());
    v
}

pub fn report(acpi: Option<&Tables>) {
    let mut s = Serial::new(serial::COM1);
    let (_, v0b, v0c, v0d) = cpuid(0, 0);
    let intel = v0b == 0x756e_6547 && v0d == 0x4965_6e69 && v0c == 0x6c65_746e;
    let (_, _, c1, _) = cpuid(1, 0);
    let hypervisor = c1 & (1 << 31) != 0;
    let vmx = intel && c1 & (1 << 5) != 0;

    let (max_ext, _, _, _) = cpuid(0x8000_0000, 0);
    let (svm, npt) = if max_ext >= 0x8000_0001 {
        let (_, _, c, _) = cpuid(0x8000_0001, 0);
        let svm = c & (1 << 2) != 0;
        let npt = if max_ext >= 0x8000_000A {
            let (_, _, _, d) = cpuid(0x8000_000A, 0);
            d & 1 != 0
        } else {
            false
        };
        (svm, npt)
    } else {
        (false, false)
    };

    // VMX guarantees the two MSRs below exist; reading them on a CPU without
    // VMX (or a non-Intel one) can raise #GP, so they are feature-gated.
    let (ept, vpid, fc_lock, fc_vmx) = if vmx {
        let fc = rdmsr(MSR_FEATURE_CONTROL);
        let cap = rdmsr(MSR_EPT_VPID_CAP);
        (cap & 1 != 0, cap & (1 << 16) != 0, fc & 1 != 0, fc & 2 != 0)
    } else {
        (false, false, false, false)
    };

    let vendor = if hypervisor { Some(hypervisor_vendor()) } else { None };
    let tcg = vendor.map_or(false, |v| &v == b"TCGTCGTCGTCG");
    let _ = write!(s, "virt: ");
    if let Some(v) = &vendor {
        let _ = s.write(b"guest under ");
        // The vendor field is a fixed 12 bytes padded with NULs (KVM sends
        // "KVMKVMKVM\0\0\0"). Writing them raw put NUL bytes into every
        // serial log, which makes `grep` treat the whole file as binary and
        // silently fail the smoke gates' plain `grep -q` marker checks.
        let end = v.iter().position(|&b| b == 0).unwrap_or(v.len());
        let _ = s.write(&v[..end]);
        let _ = write!(s, "  ");
    } else {
        let _ = write!(s, "bare metal  ");
    }
    let _ = write!(s, "vmx={} svm={} ept={} npt={}", vmx, svm, ept, npt);
    let iommu = match acpi {
        Some(t) => {
            let _ = writeln!(s, " iommu: dmar={} ivrs={}", t.has_dmar, t.has_ivrs);
            t.has_dmar || t.has_ivrs
        }
        None => {
            let _ = writeln!(s, " iommu: unknown (no ACPI)");
            false
        }
    };
    if vmx {
        let _ = writeln!(s, "virt: vmx-msr lock={} enabled={} vpid={}", fc_lock, fc_vmx, vpid);
    }
    // ROADMAP sec. 9 rows: name the row this machine lands in so the M15
    // disk-service VM can decide without re-deriving it.
    let row = if hypervisor {
        "now (run-as-guest path)"
    } else if vmx || svm {
        if iommu {
            "2006-2010 (VT-x/EPT or AMD-V/NPT + VT-d/AMD-Vi)"
        } else {
            "2006-2009 (VT-x/EPT or AMD-V/NPT)"
        }
    } else {
        "pre-2006 (no hardware virtualization; physical-mount fallback)"
    };
    let _ = writeln!(s, "virt: matrix row {}", row);
    // 2010-era to modern: with neither VMX nor SVM there is no hardware
    // virtualization, so M15's disk-service VM must fall back to a physical
    // mount; with one of them (and an IOMMU for device isolation) the V2/V3
    // hypervisor path applies. The IOMMU presence above gates its claims.
    let _ = writeln!(
        s,
        "virt: capability {}",
        if vmx || svm {
            if tcg { "hardware-assisted (TCG-emulated; nested use unverified)" } else { "hardware-assisted" }
        } else {
            "none (physical mount fallback)"
        }
    );
}
