//! Program break (brk) for the libc allocator (docs/POSIX_PLAN.md).
//!
//! The user heap starts at the ELF-derived base (P2; `USER_HEAP_BASE` when
//! the image does not say otherwise) and grows upward in 4K pages allocated
//! from the frame allocator and mapped RW through the installed user ops.
//! Shrinking only moves the logical break; pages stay mapped until the task
//! is reaped (free_user_vm frees them), so a later growth never double-maps.
//! A per-task slot reset at registration keys the state to the address
//! space; fork clones the trio so the child continues from the same break.

use fantuan_abi::{SYS_ERR_INVAL, SYS_ERR_NOMEM, USER_HEAP_BASE};

use crate::frame;
use crate::task;
use crate::user::{self, Prot};

static mut BASE: [u64; task::MAX_TASKS] = [0; task::MAX_TASKS];
static mut BREAK: [u64; task::MAX_TASKS] = [0; task::MAX_TASKS];
static mut MAPPED: [u64; task::MAX_TASKS] = [0; task::MAX_TASKS];

/// Reset the heap state for a task slot (called on every user registration).
pub fn init_task(slot: usize, base: u64) {
    let base = if base < USER_HEAP_BASE { USER_HEAP_BASE } else { (base + 0xFFF) & !0xFFF };
    unsafe {
        BASE[slot] = base;
        BREAK[slot] = base;
        MAPPED[slot] = base;
    }
}

/// Fork: the child inherits the parent's heap geometry.
pub fn clone_task(parent: usize, child: usize) {
    unsafe {
        BASE[child] = BASE[parent];
        BREAK[child] = BREAK[parent];
        MAPPED[child] = MAPPED[parent];
    }
}

/// brk(0) reports the break; brk(addr) sets it, mapping new pages on growth.
pub fn brk(addr: u64) -> u64 {
    let slot = task::current_slot();
    unsafe {
        if BREAK[slot] == 0 {
            init_task(slot, 0);
        }
        if addr == 0 || addr == BREAK[slot] {
            return BREAK[slot];
        }
        if addr < BASE[slot] {
            return SYS_ERR_INVAL;
        }
        if addr > MAPPED[slot] {
            let root = task::current_vm_root();
            let ops = user::ops();
            let end = (addr + 0xFFF) & !0xFFF;
            let mut va = MAPPED[slot] & !0xFFF;
            while va < end {
                let Some(f) = frame::get().alloc() else {
                    return SYS_ERR_NOMEM;
                };
                (ops.map)(root, va, f, Prot::Rw);
                let dst = (ops.phys_to_virt)(f) as *mut u8;
                core::ptr::write_bytes(dst, 0, 4096);
                MAPPED[slot] = va + 4096;
                va += 4096;
            }
        }
        BREAK[slot] = addr;
        BREAK[slot]
    }
}

/// Heap base of a slot (used by exec/spawn bookkeeping).
pub fn base_of(slot: usize) -> u64 {
    unsafe { *core::ptr::addr_of!(BASE[slot]) }
}
