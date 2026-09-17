//! Minimal CPU-state helpers — arch code, not drivers (DESIGN.md §2.1).

use core::arch::asm;

#[inline]
pub fn sti() {
    unsafe { asm!("sti", options(nomem, preserves_flags)) }
}

/// Save RFLAGS and disable interrupts. Pair with irq_restore.
#[inline]
pub fn irq_save() -> u64 {
    let flags: u64;
    unsafe {
        asm!("pushfq; cli; pop {}", out(reg) flags, options(nomem));
    }
    flags
}

#[inline]
pub fn irq_restore(flags: u64) {
    unsafe {
        asm!("push {}; popfq", in(reg) flags, options(nomem));
    }
}

// --- M8.3c memory hardening: NX, SMEP/SMAP --------------------------------

/// CPUID (leaf, subleaf) -> (eax, ebx, ecx, edx).
#[inline]
pub fn cpuid(leaf: u32, subleaf: u32) -> (u32, u32, u32, u32) {
    let mut a = leaf;
    let mut c = subleaf;
    let b: u32;
    let d: u32;
    unsafe {
        asm!(
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

/// Does this CPU support NX (EFER.NXE), SMEP and SMAP?
pub struct Features {
    pub nx: bool,
    pub smep: bool,
    pub smap: bool,
}

pub fn features() -> Features {
    let (_, _, _, d_ext) = cpuid(0x8000_0001, 0);
    let (_, b7, _, _) = cpuid(7, 0);
    Features {
        nx: d_ext & (1 << 20) != 0,
        smep: b7 & (1 << 7) != 0,
        smap: b7 & (1 << 20) != 0,
    }
}

#[inline]
fn cr4_read() -> u64 {
    let v: u64;
    unsafe { asm!("mov {}, cr4", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

#[inline]
fn cr4_write(v: u64) {
    unsafe { asm!("mov cr4, {}", in(reg) v, options(nostack)) };
}

/// Enable EFER.NXE (IA32_EFER bit 11). Must happen BEFORE any page-table
/// entry sets the NX bit, or the CPU raises a reserved-bit page fault.
pub fn enable_nxe() -> bool {
    const IA32_EFER: u32 = 0xC000_0080;
    if !features().nx {
        return false;
    }
    let lo: u32;
    let hi: u32;
    unsafe {
        asm!("rdmsr", in("ecx") IA32_EFER, out("eax") lo, out("edx") hi,
             options(nomem, preserves_flags));
        let lo = lo | (1 << 11);
        asm!("wrmsr", in("ecx") IA32_EFER, in("eax") lo, in("edx") hi,
             options(nomem, preserves_flags));
    }
    true
}

/// Set a CR4 bit; returns false when the CPU lacks the feature.
fn cr4_set(bit: u64) -> bool {
    let before = cr4_read();
    cr4_write(before | (1 << bit));
    cr4_read() & (1 << bit) != 0
}

/// Supervisor Mode Execution Prevention.
pub fn enable_smep() -> bool {
    features().smep && cr4_set(20)
}

/// Supervisor Mode Access Prevention. Kernel code touching user pages must
/// bracket the access with stac()/clac().
pub fn enable_smap() -> bool {
    features().smap && cr4_set(21)
}

/// Set/clear the AC flag: allows supervisor access to user pages under SMAP.
#[inline]
pub fn stac() {
    unsafe { asm!("stac", options(nomem, nostack, preserves_flags)) }
}

#[inline]
pub fn clac() {
    unsafe { asm!("clac", options(nomem, nostack, preserves_flags)) }
}
