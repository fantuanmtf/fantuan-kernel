//! Sv39 page tables (M9.1b): identity + PHYS_OFFSET alias, 2 MiB leaves for
//! RAM, 4 KiB leaves for MMIO, satp enable and the high-half jump helper.
//!
//! Until satp is written the CPU runs in bare mode (VA = PA), so table
//! construction accesses frames PHYSICALLY; `use_alias()` flips the access
//! base once the kernel runs from the high half. Temporary local copy until
//! M9.2a extracts the shared allocator/paging interfaces into kernel-core.

use core::arch::{asm, global_asm};

use crate::frame;

/// Sv39-canonical high-half base (DESIGN.md §14.2). Not the x86 value: the
/// ABI's PHYS_OFFSET becomes cfg-dependent when the crates share it (M9.2a).
pub const PHYS_OFFSET: u64 = 0xFFFF_FFC0_0000_0000;

pub const fn phys_to_virt(p: u64) -> u64 {
    PHYS_OFFSET + p
}

const PTE_V: u64 = 1 << 0;
const PTE_R: u64 = 1 << 1;
const PTE_W: u64 = 1 << 2;
const PTE_X: u64 = 1 << 3;
const PTE_G: u64 = 1 << 5;
const PTE_A: u64 = 1 << 6;
const PTE_D: u64 = 1 << 7;
const SATP_SV39: u64 = 8 << 60;

/// Access base: 0 while bare, PHYS_OFFSET after the high-half jump.
static mut ACCESS: u64 = 0;
static mut ROOT: u64 = 0;

pub fn kernel_root() -> u64 {
    unsafe { ROOT }
}

/// Switch table accesses to the alias (call once running in the high half).
pub fn use_alias() {
    unsafe {
        let root = ROOT;
        ACCESS = PHYS_OFFSET;
        let _ = root;
    }
}

fn at(phys: u64) -> *mut u64 {
    unsafe { (phys + ACCESS) as *mut u64 }
}

fn zero_frame(phys: u64) {
    let p = at(phys);
    for i in 0..512 {
        unsafe { *p.add(i) = 0 };
    }
}

fn entry_ptr(table_phys: u64, index: usize) -> *mut u64 {
    unsafe { at(table_phys).add(index) }
}

fn ppn(pte: u64) -> u64 {
    pte >> 10
}

/// Get (allocating when empty) the next-level table for INDEX.
fn table(parent_phys: u64, index: usize) -> u64 {
    let e = unsafe { *entry_ptr(parent_phys, index) };
    if e & PTE_V != 0 {
        return ppn(e) << 12;
    }
    let f = frame::get().alloc().expect("no frame for an Sv39 table");
    zero_frame(f);
    unsafe { *entry_ptr(parent_phys, index) = ((f >> 12) << 10) | PTE_V };
    f
}

/// Build a fresh root table; returns its physical address.
pub fn init() -> u64 {
    let root = frame::get().alloc().expect("no frame for the Sv39 root");
    zero_frame(root);
    unsafe { ROOT = root };
    root
}

fn leaf(va: u64, pa: u64, flags: u64, level1: bool) {
    let root = kernel_root();
    let i2 = ((va >> 30) & 0x1FF) as usize;
    let pd = table(root, i2);
    if level1 {
        let i1 = ((va >> 21) & 0x1FF) as usize;
        unsafe { *entry_ptr(pd, i1) = ((pa >> 12) << 10) | flags | PTE_V | PTE_A | PTE_D };
    } else {
        let i1 = ((va >> 21) & 0x1FF) as usize;
        let pt = table(pd, i1);
        let i0 = ((va >> 12) & 0x1FF) as usize;
        unsafe { *entry_ptr(pt, i0) = ((pa >> 12) << 10) | flags | PTE_V | PTE_A | PTE_D };
    }
}

/// Map a 2 MiB-aligned physical range at the same virtual address (identity
/// plus alias is up to the caller).
pub fn map_2m(va: u64, pa: u64, flags: u64) {
    leaf(va & !(2 * 1024 * 1024 - 1), pa & !(2 * 1024 * 1024 - 1), flags, true);
}

pub fn map_4k(va: u64, pa: u64, flags: u64) {
    leaf(va & !0xFFF, pa & !0xFFF, flags, false);
}

pub fn ram_flags() -> u64 {
    PTE_R | PTE_W | PTE_X | PTE_G
}

pub fn mmio_flags() -> u64 {
    PTE_R | PTE_W | PTE_G
}

/// Write satp (Sv39) and flush the TLB.
pub fn enable() {
    let root = kernel_root();
    let satp = SATP_SV39 | (root >> 12);
    unsafe {
        asm!("csrw satp, {}", in(reg) satp, options(nostack));
        asm!("sfence.vma", options(nostack));
    }
}

global_asm!(
    ".section .text",
    ".global riscv_jump",
    ".type riscv_jump, @function",
    "riscv_jump:",
    "    jr a0",
);

extern "C" {
    /// Jump to TARGET (used once, after satp, with sp already on the alias).
    pub fn riscv_jump(target: usize) -> !;
}
