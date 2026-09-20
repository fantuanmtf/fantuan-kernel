//! Program break (brk) for the P1 libc allocator (docs/POSIX_PLAN.md).
//!
//! The user heap starts at `fantuan_abi::USER_HEAP_BASE` (0x500000; the ELF
//! images link at 0x400000 and are tiny) and grows upward in 4K pages
//! allocated from the frame allocator and mapped RW through the installed
//! user ops. Shrinking only moves the logical break; pages stay mapped until
//! the task is reaped (free_user_vm frees them), so a later growth never
//! double-maps. A per-task slot reset at registration keys the state to the
//! address space.

use fantuan_abi::{SYS_ERR_INVAL, SYS_ERR_NOMEM, USER_HEAP_BASE};

use crate::frame;
use crate::task;
use crate::user::{self, Prot};

static mut BREAK: [u64; task::MAX_TASKS] = [0; task::MAX_TASKS];
static mut MAPPED: [u64; task::MAX_TASKS] = [0; task::MAX_TASKS];

/// Reset the heap state for a task slot (called on every user registration).
pub fn init_task(slot: usize) {
    unsafe {
        BREAK[slot] = USER_HEAP_BASE;
        MAPPED[slot] = USER_HEAP_BASE;
    }
}

/// brk(0) reports the break; brk(addr) sets it, mapping new pages on growth.
pub fn brk(addr: u64) -> u64 {
    let slot = task::current_slot();
    unsafe {
        if BREAK[slot] == 0 {
            init_task(slot);
        }
        if addr == 0 || addr == BREAK[slot] {
            return BREAK[slot];
        }
        if addr < USER_HEAP_BASE {
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
