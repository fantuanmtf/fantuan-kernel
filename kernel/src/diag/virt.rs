//! Virtualization detection v1 (M12-7): what the CPU and firmware offer,
//! printed as a short verdict plus the compatibility facts M14/M15 need.
//! Read-only: CPUID and the ACPI tables only; no MSR is touched unless the
//! CPUID feature that guarantees it is present.

use core::fmt::Write;

use crate::acpi::Tables;
use crate::cpu::cpuid;
use crate::serial::{self, Serial};

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
    let (_, _, c1, _) = cpuid(1, 0);
    let hypervisor = c1 & (1 << 31) != 0;
    let vmx = c1 & (1 << 5) != 0;

    let (_, _, c8, _) = cpuid(0x8000_0000, 0);
    let svm = if c8 >= 0x8000_0001 {
        let (_, _, c, _) = cpuid(0x8000_0001, 0);
        c & (1 << 2) != 0
    } else {
        false
    };

    let _ = write!(s, "virt: ");
    if hypervisor {
        let v = hypervisor_vendor();
        let _ = s.write(b"guest under ");
        let _ = s.write(&v);
        let _ = write!(s, "  ");
    } else {
        let _ = write!(s, "bare metal  ");
    }
    let _ = write!(s, "vmx={} svm={}", vmx, svm);
    match acpi {
        Some(t) => {
            let _ = writeln!(s, " iommu: dmar={} ivrs={}", t.has_dmar, t.has_ivrs);
        }
        None => {
            let _ = writeln!(s, " iommu: unknown (no ACPI)");
        }
    }
    // 2010-era to modern: with neither VMX nor SVM there is no hardware
    // virtualization, so M15's disk-service VM must fall back to a physical
    // mount; with one of them (and an IOMMU for device isolation) the V2/V3
    // hypervisor path applies. The IOMMU presence above gates its claims.
    let _ = writeln!(
        s,
        "virt: capability {}",
        if vmx || svm { "hardware-assisted" } else { "none (physical mount fallback)" }
    );
}
