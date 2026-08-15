//! User-space page tables (M4, DESIGN.md §4.6).
//!
//! Every user task gets its own PML4: the kernel half is cloned from the
//! kernel's tables (identity 4 GiB + PHYS_OFFSET alias), the user half starts
//! empty and is filled by the ELF loader. M4 simplification: user pages are
//! RWX (no NX/SMEP/SMAP yet) and everything is 4K-mapped.

use crate::mm::frame;
use crate::mm::paging::{self, phys_to_virt};

pub const P_PRESENT: u64 = 1;
pub const P_WRITABLE: u64 = 1 << 1;
pub const P_USER: u64 = 1 << 2;

const ADDR_MASK: u64 = 0x000F_FFFF_FFFF_F000;

/// A fresh PML4 for a user task: the kernel half (PHYS_OFFSET alias) is
/// cloned in, the user half starts EMPTY. The kernel's identity map is
/// deliberately NOT cloned — its 2 MiB huge pages would block map_page from
/// building user 4K mappings in the low half (a real bug we hit in M4).
pub fn new_user_pml4() -> u64 {
    let pml4_phys = frame::get().alloc().expect("no frame for user PML4");
    let pml4 = phys_to_virt(pml4_phys) as *mut u64;
    let kern = phys_to_virt(paging::kernel_pml4()) as *const u64;
    unsafe {
        for i in 0..512 {
            *pml4.add(i) = 0;
        }
        *pml4.add(256) = *kern.add(256); // PHYS_OFFSET alias (kernel half)
    }
    pml4_phys
}

/// Map one 4K page into the given page-table tree, creating the intermediate
/// levels on demand (from the frame allocator). flags: P | W | U.
pub fn map_page(cr3: u64, vaddr: u64, phys: u64, flags: u64) {
    unsafe {
        let pml4 = phys_to_virt(cr3) as *mut u64;
        let i4 = ((vaddr >> 39) & 0x1FF) as usize;
        let pdpt = next_level(pml4.add(i4));
        let i3 = ((vaddr >> 30) & 0x1FF) as usize;
        let pd = next_level(pdpt.add(i3));
        let i2 = ((vaddr >> 21) & 0x1FF) as usize;
        let pt = next_level(pd.add(i2));
        let i1 = ((vaddr >> 12) & 0x1FF) as usize;
        *pt.add(i1) = phys | flags;
    }
}

/// Return the next table level, allocating a frame when the entry is empty.
/// Intermediate levels are user-accessible so the whole path works for the
/// user half (the kernel half is never touched through this API).
unsafe fn next_level(entry: *mut u64) -> *mut u64 {
    if *entry & P_PRESENT == 0 {
        let f = frame::get().alloc().expect("no frames for user page tables");
        *entry = f | P_PRESENT | P_WRITABLE | P_USER;
    }
    phys_to_virt(*entry & ADDR_MASK) as *mut u64
}
