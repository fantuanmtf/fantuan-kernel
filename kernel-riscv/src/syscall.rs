//! riscv64 syscall bridge (M9.3c): the entry is `ecall` from U-mode (taken in
//! trap.rs); the semantics live in kernel-core::syscall. This file owns the
//! user-pointer write bridge: SUM (sstatus bit 18) allows S-mode reads from U
//! pages, and a fault while IN_USER_COPY is set is classified as a bad user
//! pointer (kills the task) instead of a kernel bug.

use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

use fantuan_abi::{SYS_ERR_INVAL, SYS_OK};

static IN_USER_COPY: AtomicBool = AtomicBool::new(false);

pub fn in_user_copy() -> bool {
    IN_USER_COPY.load(Ordering::Relaxed)
}

pub fn clear_user_copy() {
    IN_USER_COPY.store(false, Ordering::Relaxed);
}

fn set_sum(on: bool) {
    unsafe {
        if on {
            asm!("csrs sstatus, {}", in(reg) 1u64 << 18, options(nostack));
        } else {
            asm!("csrc sstatus, {}", in(reg) 1u64 << 18, options(nostack));
        }
    }
}

/// SYS_WRITE bridge: bounded copy from user memory to the UART.
pub fn write(ptr: u64, len: u64) -> u64 {
    let len = len.min(512);
    if ptr == 0 || ptr >= crate::paging::PHYS_OFFSET || ptr + len > crate::paging::PHYS_OFFSET {
        return SYS_ERR_INVAL;
    }
    // User pages exist only in the task's root: enter it for the copy and
    // restore the kernel root (the caller's invariant) afterwards.
    crate::paging::set_root(kernel_core::task::current_vm_root());
    IN_USER_COPY.store(true, Ordering::Relaxed);
    set_sum(true);
    let buf = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    for &b in buf {
        crate::uart_putc(b);
    }
    set_sum(false);
    IN_USER_COPY.store(false, Ordering::Relaxed);
    crate::paging::set_root(crate::paging::kernel_root());
    SYS_OK
}
