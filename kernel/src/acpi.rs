//! Minimal ACPI table walker (M12-1): validates the RSDP, walks the
//! RSDT/XSDT and reports the tables the later milestones need — FADT (power
//! management flags), MADT (CPU count) and the IOMMU tables (DMAR/IVRS) for
//! the virtualization work. Read-only and bounded: every structure is
//! checksum-verified and every length is clamped before it is trusted.

use core::fmt::Write;

use crate::mm::paging::phys_to_virt;
use crate::serial::{self, Serial};

const RSDP_SIG: &[u8; 8] = b"RSD PTR ";
const MAX_TABLES: usize = 64;

#[allow(dead_code)] // has_fadt/cpu_count reserved for M12-6 (thermal)
pub struct Tables {
    pub revision: u8,
    pub count: usize,
    pub has_fadt: bool,
    pub has_madt: bool,
    pub has_dmar: bool,
    pub has_ivrs: bool,
    pub cpu_count: u32,
}

fn bytes(phys: u64, len: usize) -> &'static [u8] {
    unsafe { core::slice::from_raw_parts(phys_to_virt(phys) as *const u8, len) }
}

fn checksum(data: &[u8]) -> bool {
    data.iter().fold(0u8, |a, b| a.wrapping_add(*b)) == 0
}

/// ACPI scalars (table pointers, lengths, entry addresses) are little-endian.
fn le32(d: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

fn le64(d: &[u8], off: usize) -> u64 {
    let mut v = 0u64;
    for i in 0..8 {
        v |= (d[off + i] as u64) << (8 * i);
    }
    v
}

/// Walk the tables at the RSDP physical address; None when absent/invalid.
pub fn init(rsdp_phys: u64) -> Option<Tables> {
    if rsdp_phys == 0 {
        return None;
    }
    let rsdp = bytes(rsdp_phys, 36);
    if &rsdp[0..8] != RSDP_SIG || !checksum(&rsdp[..20]) {
        let mut s = Serial::new(serial::COM1);
        let _ = writeln!(s, "acpi: bad RSDP at {:#x}", rsdp_phys);
        return None;
    }
    let revision = rsdp[15];
    let rsdt = le32(rsdp, 16) as u64;
    let xsdt = if revision >= 2 { le64(rsdp, 24) } else { 0 };

    // Prefer the XSDT on ACPI 2.0+ when it is present and checksum-valid.
    let mut use64 = false;
    let mut sdt = rsdt;
    if xsdt != 0 {
        let len = le32(bytes(xsdt, 8), 4) as usize;
        if (36..=1024 * 1024).contains(&len) && checksum(bytes(xsdt, len)) {
            sdt = xsdt;
            use64 = true;
        }
    }
    if sdt == 0 {
        let mut s = Serial::new(serial::COM1);
        let _ = writeln!(s, "acpi: no RSDT/XSDT");
        return None;
    }
    let len = le32(bytes(sdt, 8), 4) as usize;
    if !(36..=1024 * 1024).contains(&len) || !checksum(bytes(sdt, len)) {
        let mut s = Serial::new(serial::COM1);
        let _ = writeln!(s, "acpi: bad RSDT/XSDT");
        return None;
    }
    let entry = if use64 { 8 } else { 4 };
    let count = ((len - 36) / entry).min(MAX_TABLES);
    let sdt_bytes = bytes(sdt, len);

    let mut t = Tables {
        revision,
        count: 0,
        has_fadt: false,
        has_madt: false,
        has_dmar: false,
        has_ivrs: false,
        cpu_count: 0,
    };
    let mut found = [0u8; 4 * 8];
    let mut found_n = 0usize;
    for i in 0..count {
        let off = 36 + i * entry;
        let table = if use64 { le64(sdt_bytes, off) } else { le32(sdt_bytes, off) as u64 };
        if table == 0 {
            continue;
        }
        let hdr = bytes(table, 36);
        let hlen = le32(hdr, 4) as usize;
        if !(36..=4096).contains(&hlen) || !checksum(bytes(table, hlen)) {
            continue;
        }
        let sig = [hdr[0], hdr[1], hdr[2], hdr[3]];
        t.count += 1;
        match &sig {
            b"FACP" => t.has_fadt = true,
            b"APIC" => {
                t.has_madt = true;
                // MADT: local APIC entries (type 0) with the enabled bit set.
                let m = bytes(table, hlen);
                let mut p = 44;
                while p + 2 <= hlen {
                    let etype = m[p];
                    let elen = m[p + 1] as usize;
                    if elen < 2 || p + elen > hlen {
                        break;
                    }
                    if etype == 0 && elen >= 8 && m[p + 4] & 1 != 0 {
                        t.cpu_count += 1;
                    }
                    p += elen;
                }
            }
            b"DMAR" => t.has_dmar = true,
            b"IVRS" => t.has_ivrs = true,
            _ => {}
        }
        if found_n < 8 {
            found[found_n * 4..found_n * 4 + 4].copy_from_slice(&sig);
            found_n += 1;
        }
    }

    let mut s = Serial::new(serial::COM1);
    let _ = writeln!(
        s,
        "acpi: rev {} {} ({} tables)",
        revision,
        if use64 { "XSDT" } else { "RSDT" },
        t.count
    );
    for i in 0..found_n {
        let _ = write!(s, "acpi: table ");
        let _ = s.write(&found[i * 4..i * 4 + 4]);
        let _ = writeln!(s);
    }
    let _ = writeln!(
        s,
        "acpi: fadt={} madt={} cpus={} dmar={} ivrs={}",
        t.has_fadt, t.has_madt, t.cpu_count, t.has_dmar, t.has_ivrs
    );
    Some(t)
}
