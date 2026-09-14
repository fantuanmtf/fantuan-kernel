//! Kernel-owned page tables (DESIGN.md §4.5): rebuild the bootloader's
//! identity-4GiB + PHYS_OFFSET map from allocator frames, switch CR3, and let
//! the caller reclaim the bootloader's tables.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::mm::frame::FrameAllocator;
use fantuan_abi::PHYS_OFFSET;

/// Physical address of the kernel's current PML4 (set by init; cloned into
/// every user task's page tables).
static KERNEL_PML4: AtomicU64 = AtomicU64::new(0);

pub fn kernel_pml4() -> u64 {
    KERNEL_PML4.load(Ordering::Relaxed)
}

const HUGE: u64 = 2 * 1024 * 1024;
const IDENT_GIB: u64 = 4;

const P_PRESENT: u64 = 1;
const P_WRITABLE: u64 = 1 << 1;
const P_HUGE: u64 = 1 << 7;

const PML4_IDENT: usize = 0;
const PML4_OFFSET: usize = ((PHYS_OFFSET >> 39) & 0x1FF) as usize; // 256

/// The canonical physical-to-virtual conversion for the M2 map.
pub const fn phys_to_virt(phys: u64) -> u64 {
    PHYS_OFFSET + phys
}

const P_PCD: u64 = 1 << 4; // cache disable (MMIO)
const P_PWT: u64 = 1 << 3; // write-through (MMIO)

/// Map a physical MMIO range into the PHYS_OFFSET window with 2 MiB pages,
/// creating the page-table levels on demand. Modern NVMe controllers expose
/// 64-bit BARs above 4 GiB, which the boot map does not cover — this is what
/// makes such devices reachable. Returns the virtual base address.
pub fn map_mmio(alloc: &mut FrameAllocator, phys: u64, len: u64) -> Option<u64> {
    if len == 0 {
        return None;
    }
    let pml4_phys = KERNEL_PML4.load(Ordering::Relaxed);
    if pml4_phys == 0 {
        return None;
    }
    let start = phys & !(HUGE - 1);
    let end = (phys + len + HUGE - 1) & !(HUGE - 1);

    unsafe {
        let pml4 = phys_to_virt(pml4_phys) as *mut u64;
        let mut p = start;
        while p < end {
            let v = phys_to_virt(p);
            let i4 = ((v >> 39) & 0x1FF) as usize;
            let i3 = ((v >> 30) & 0x1FF) as usize;
            let i2 = ((v >> 21) & 0x1FF) as usize;

            if *pml4.add(i4) & P_PRESENT == 0 {
                let f = alloc.alloc()?;
                let t = phys_to_virt(f) as *mut u64;
                for k in 0..512 {
                    *t.add(k) = 0;
                }
                *pml4.add(i4) = f | P_PRESENT | P_WRITABLE;
            }
            let pdpt = phys_to_virt(*pml4.add(i4) & !0xFFF) as *mut u64;
            if *pdpt.add(i3) & P_PRESENT == 0 {
                let f = alloc.alloc()?;
                let t = phys_to_virt(f) as *mut u64;
                for k in 0..512 {
                    *t.add(k) = 0;
                }
                *pdpt.add(i3) = f | P_PRESENT | P_WRITABLE;
            }
            let pd = phys_to_virt(*pdpt.add(i3) & !0xFFF) as *mut u64;
            *pd.add(i2) = p | P_PRESENT | P_WRITABLE | P_HUGE | P_PCD | P_PWT;
            p += HUGE;
        }
        asm!("invlpg [{}]", in(reg) start + PHYS_OFFSET, options(nostack));
    }
    Some(start + PHYS_OFFSET)
}

/// Build the tables, switch CR3, and return the new PML4's physical address.
pub fn init(alloc: &mut FrameAllocator) -> u64 {
    // 1 PML4 + 2 PDPTs + 2 x 4 PDs = 11 frames.
    const N: usize = 11;
    let mut frames = [0u64; N];
    for f in frames.iter_mut() {
        *f = alloc.alloc().expect("out of frames while building page tables");
    }
    let pml4_phys = frames[0];

    unsafe {
        // Write through the PHYS_OFFSET alias, but page-table entries must
        // hold PHYSICAL addresses (the CPU walks them as physical pointers).
        let pml4 = phys_to_virt(frames[0]) as *mut u64;
        let pdpt_id = phys_to_virt(frames[1]) as *mut u64;
        let pdpt_hi = phys_to_virt(frames[2]) as *mut u64;
        for i in 0..512 {
            *pml4.add(i) = 0;
            *pdpt_id.add(i) = 0;
            *pdpt_hi.add(i) = 0;
        }
        for g in 0..IDENT_GIB {
            let pd_id_phys = frames[3 + g as usize];
            let pd_hi_phys = frames[7 + g as usize];
            let pd_id = phys_to_virt(pd_id_phys) as *mut u64;
            let pd_hi = phys_to_virt(pd_hi_phys) as *mut u64;
            for j in 0..512u64 {
                let phys = g * 0x4000_0000 + j * HUGE;
                let entry = phys | P_PRESENT | P_WRITABLE | P_HUGE;
                *pd_id.add(j as usize) = entry;
                *pd_hi.add(j as usize) = entry;
            }
            *pdpt_id.add(g as usize) = pd_id_phys | P_PRESENT | P_WRITABLE;
            *pdpt_hi.add(g as usize) = pd_hi_phys | P_PRESENT | P_WRITABLE;
        }
        *pml4.add(PML4_IDENT) = frames[1] | P_PRESENT | P_WRITABLE;
        *pml4.add(PML4_OFFSET) = frames[2] | P_PRESENT | P_WRITABLE;

        // Identical content to the bootloader's map: switching is invisible.
        asm!("mov cr3, {}", in(reg) pml4_phys, options(nostack));
    }

    KERNEL_PML4.store(pml4_phys, Ordering::Relaxed);
    pml4_phys
}
