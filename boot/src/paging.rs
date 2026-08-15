//! Initial page tables: identity-map the first 4 GiB and alias it at
//! PHYS_OFFSET with 2 MiB huge pages, so the higher-half kernel can boot
//! while pre-existing physical pointers (framebuffer, EFI structures) keep
//! working (DESIGN.md §4.5). The kernel rebuilds an equivalent set from its
//! own frame allocator right after boot.

use core::arch::asm;

use crate::console;
use crate::uefi::protocol::SimpleTextOutput;
use crate::uefi::table::BootServices;
use crate::uefi::{ALLOCATE_MAX_ADDRESS, EFI_LOADER_DATA, EFI_SUCCESS};
use fantuan_abi::PHYS_OFFSET;

const PAGE: u64 = 4096;
const HUGE: u64 = 2 * 1024 * 1024;
const IDENT_GIB: u64 = 4; // identity-mapped span

const P_PRESENT: u64 = 1;
const P_WRITABLE: u64 = 1 << 1;
const P_HUGE: u64 = 1 << 7;

const PML4_IDENT: usize = 0;
const PML4_OFFSET: usize = ((PHYS_OFFSET >> 39) & 0x1FF) as usize; // 256

/// 1 PML4 + 2 PDPTs + 2 x 4 PDs.
const TABLE_PAGES: u64 = 11;

#[derive(Clone, Copy)]
pub struct TablePages {
    pub pml4: u64,
    pub pages: u64,
}

/// Allocate the table pages BELOW the kernel image (the firmware must never
/// hand out pages inside the kernel we are about to load).
pub fn allocate_tables(bs: &BootServices, con: *mut SimpleTextOutput, kernel_addr: u64) -> Option<TablePages> {
    let mut base: u64 = kernel_addr; // in: max address; out: allocated
    let sts = (bs.allocate_pages)(ALLOCATE_MAX_ADDRESS, EFI_LOADER_DATA, TABLE_PAGES as usize, &mut base);
    if sts != EFI_SUCCESS {
        console::println(con, "ERROR: page-table allocation failed");
        return None;
    }
    Some(TablePages { pml4: base, pages: TABLE_PAGES })
}

/// Fill the tables and switch CR3. Runs AFTER ExitBootServices (plain memory
/// writes + a CR3 load; no boot services involved).
pub fn enable(t: &TablePages) {
    unsafe {
        let pml4 = t.pml4 as *mut u64;
        let pdpt_id = (t.pml4 + PAGE) as *mut u64;
        let pdpt_hi = (t.pml4 + 2 * PAGE) as *mut u64;

        // Fresh pages hold garbage: zero every entry first.
        for i in 0..512 {
            *pml4.add(i) = 0;
            *pdpt_id.add(i) = 0;
            *pdpt_hi.add(i) = 0;
        }

        for g in 0..IDENT_GIB {
            let pd_id = (t.pml4 + (3 + g) * PAGE) as *mut u64;
            let pd_hi = (t.pml4 + (3 + IDENT_GIB + g) * PAGE) as *mut u64;
            for j in 0..512u64 {
                let phys = g * 0x4000_0000 + j * HUGE;
                let entry = phys | P_PRESENT | P_WRITABLE | P_HUGE;
                *pd_id.add(j as usize) = entry;
                *pd_hi.add(j as usize) = entry;
            }
            *pdpt_id.add(g as usize) = pd_id as u64 | P_PRESENT | P_WRITABLE;
            *pdpt_hi.add(g as usize) = pd_hi as u64 | P_PRESENT | P_WRITABLE;
        }

        *pml4.add(PML4_IDENT) = pdpt_id as u64 | P_PRESENT | P_WRITABLE;
        *pml4.add(PML4_OFFSET) = pdpt_hi as u64 | P_PRESENT | P_WRITABLE;

        // Execution continues on the identity map; the kernel is entered at
        // PHYS_OFFSET + kernel_base by the caller.
        asm!("mov cr3, {}", in(reg) t.pml4, options(nostack));
    }
}
