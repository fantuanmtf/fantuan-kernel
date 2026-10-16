//! The per-process address space: the VMA list and mmap/munmap/mprotect.
//! M14-1's demand paging and COW land here. A child of `process`; see
//! `super` for the table and locking model.

use super::{lock, procs, Proc, Vma};
use fantuan_abi::{
    MAP_ANONYMOUS, MAP_FIXED, MAP_SHARED, SYS_ERR_INVAL, SYS_ERR_NOMEM, SYS_ERR_NOSYS, SYS_OK,
};
use crate::task;
use crate::user;

pub fn vma_clear(slot: usize) {
    let _g = lock();
    procs()[slot].vma_n = 0;
    procs()[slot].mmap_next = fantuan_abi::USER_MMAP_BASE;
}

fn vma_overlaps(p: &Proc, start: u64, end: u64) -> bool {
    p.vmas[..p.vma_n as usize].iter().any(|v| start < v.end && end > v.start)
}

/// Anonymous private mmap. Eager frames (documented P2 tradeoff); the area
/// starts at USER_MMAP_BASE and grows upward, brk keeps its own region.
pub fn mmap2(addr: u64, len: u64, prot: u64, flags: u64, fd: u64) -> u64 {
    if len == 0 || len > (64 << 20) {
        return SYS_ERR_INVAL;
    }
    if flags & MAP_ANONYMOUS == 0 || flags & MAP_SHARED != 0 || fd as i64 >= 0 {
        return SYS_ERR_NOSYS; // no file-backed or shared mappings yet
    }
    let rounded = (len + 0xFFF) & !0xFFF;
    let slot = task::current_slot();
    let root = task::current_vm_root();
    let (base, end) = {
        let _g = lock();
        let p = &mut procs()[slot];
        let mut start = p.mmap_next & !0xFFF;
        if flags & MAP_FIXED != 0 {
            start = addr & !0xFFF;
        }
        if start < fantuan_abi::USER_MMAP_BASE || start + rounded > fantuan_abi::PHYS_OFFSET {
            return SYS_ERR_NOMEM;
        }
        let mut probe = start;
        while vma_overlaps(p, probe, probe + rounded) {
            probe += 0x1000;
        }
        if flags & MAP_FIXED != 0 && probe != start {
            return SYS_ERR_NOMEM;
        }
        start = probe;
        let end = start + rounded;
        // Reserve the range before mapping (page-table work outside the lock).
        if p.vma_n as usize >= fantuan_abi::MAX_VMAS {
            return SYS_ERR_NOMEM;
        }
        p.vmas[p.vma_n as usize] = Vma { start, end, prot };
        p.vma_n += 1;
        p.mmap_next = end;
        (start, end)
    };
    let kprot = match prot {
        0 => return SYS_ERR_INVAL,
        p if p & fantuan_abi::PROT_WRITE != 0 && p & fantuan_abi::PROT_EXEC != 0 => user::Prot::Rwx,
        p if p & fantuan_abi::PROT_WRITE != 0 => user::Prot::Rw,
        p if p & fantuan_abi::PROT_EXEC != 0 => user::Prot::Rx,
        _ => user::Prot::Ro,
    };
    let ops = user::ops();
    let mut va = base;
    while va < end {
        let Some(f) = crate::frame::get().alloc() else {
            return SYS_ERR_NOMEM;
        };
        (ops.map)(root, va, f, kprot);
        let dst = (ops.phys_to_virt)(f) as *mut u8;
        unsafe { core::ptr::write_bytes(dst, 0, 4096) };
        va += 0x1000;
    }
    base
}

/// Remove VMAs intersecting [start, end); the caller unmap is page-granular.
pub fn vma_forget(slot: usize, start: u64, end: u64) {
    let _g = lock();
    let p = &mut procs()[slot];
    let mut n = p.vma_n as usize;
    let mut i = 0;
    while i < n {
        let v = p.vmas[i];
        if v.end <= start || v.start >= end {
            i += 1;
            continue;
        }
        if v.start < start && v.end > end {
            // Split: keep [v.start, start) and [end, v.end).
            if n >= fantuan_abi::MAX_VMAS {
                break;
            }
            p.vmas[i] = Vma { start: v.start, end: start, prot: v.prot };
            p.vmas[n] = Vma { start: end, end: v.end, prot: v.prot };
            n += 1;
            i += 1;
        } else if v.start < start {
            p.vmas[i] = Vma { start: v.start, end: start, prot: v.prot };
            i += 1;
        } else if v.end > end {
            p.vmas[i] = Vma { start: end, end: v.end, prot: v.prot };
            i += 1;
        } else {
            p.vmas[i] = p.vmas[n - 1];
            n -= 1;
        }
    }
    p.vma_n = n as u8;
}

/// Update the protection of VMAs covering [start, end).
pub fn vma_reprotect(slot: usize, start: u64, end: u64, prot: u64) {
    let _g = lock();
    let p = &mut procs()[slot];
    for i in 0..p.vma_n as usize {
        let v = p.vmas[i];
        if v.start < end && v.end > start {
            p.vmas[i].prot = prot;
        }
    }
}

pub fn vma_contains(slot: usize, start: u64, end: u64) -> bool {
    let _g = lock();
    let p = procs()[slot];
    let mut cur = start;
    while cur < end {
        match p.vmas[..p.vma_n as usize].iter().find(|v| v.start <= cur && cur < v.end) {
            Some(v) => {
                let step = v.end.min(end);
                if step <= cur {
                    return false;
                }
                cur = step;
            }
            None => return false,
        }
    }
    true
}

fn kprot(prot: u64) -> Option<user::Prot> {
    if prot == 0 {
        return None;
    }
    Some(match (prot & fantuan_abi::PROT_WRITE != 0, prot & fantuan_abi::PROT_EXEC != 0) {
        (false, false) => user::Prot::Ro,
        (true, false) => user::Prot::Rw,
        (false, true) => user::Prot::Rx,
        (true, true) => user::Prot::Rwx,
    })
}

/// munmap(addr, len): unmap page-aligned addresses (frame + PTE dropped).
pub fn munmap(addr: u64, len: u64) -> u64 {
    if len == 0 {
        return SYS_ERR_INVAL;
    }
    let start = addr & !0xFFF;
    let end = (addr + len + 0xFFF) & !0xFFF;
    let slot = task::current_slot();
    if !vma_contains(slot, start, end) {
        return SYS_ERR_INVAL;
    }
    let root = task::current_vm_root();
    let ops = user::ops();
    let mut va = start;
    while va < end {
        (ops.unmap)(root, va);
        va += 0x1000;
    }
    vma_forget(slot, start, end);
    SYS_OK
}

/// mprotect(addr, len, prot): update VMAs and the live PTEs.
pub fn mprotect(addr: u64, len: u64, prot: u64) -> u64 {
    let Some(kp) = kprot(prot) else { return SYS_ERR_INVAL };
    let start = addr & !0xFFF;
    let end = (addr + len + 0xFFF) & !0xFFF;
    let slot = task::current_slot();
    if !vma_contains(slot, start, end) {
        return SYS_ERR_INVAL;
    }
    let root = task::current_vm_root();
    let ops = user::ops();
    let mut va = start;
    while va < end {
        (ops.protect)(root, va, kp);
        va += 0x1000;
    }
    vma_reprotect(slot, start, end, prot);
    SYS_OK
}
