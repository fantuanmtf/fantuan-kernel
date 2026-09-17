//! Exited-task reaping (M8.3b): free the kernel stack and the user address
//! space of tasks whose exit() marked them Exited, then release the slot.
//! Runs from schedule(), possibly in IRQ context — the frame allocator is
//! interrupt-safe since M8.3a.
//!
//! The caller's own slot is never touched: a task cannot free the stack it
//! is running on. The next schedule() call (another task, or the timer IRQ)
//! reaps it instead.

use core::fmt::Write;

use super::{State, MAX_TASKS, STACK_PAGES, TASKS};
use crate::mm::{frame, paging, user};
use crate::serial::{self, Serial};

/// Free every Exited task except the caller's slot.
pub(super) fn reap_exited(current: usize) {
    for i in 0..MAX_TASKS {
        if i == current {
            continue;
        }
        unsafe {
            let t = &mut *core::ptr::addr_of_mut!(TASKS[i]);
            if t.state != State::Exited {
                continue;
            }
            let tid = t.id;
            // Kernel stack: STACK_PAGES contiguous frames. These are NOT
            // mapped in the user page tables, so the PML4 walk below cannot
            // free them twice.
            for p in 0..STACK_PAGES {
                frame::get().free(t.stack_phys + p * frame::FRAME_SIZE);
            }
            // User address space: the user half plus its tables, then the
            // PML4 itself (the kernel half at entry 256 is shared).
            if t.is_user && t.cr3 != 0 && t.cr3 != paging::kernel_pml4() {
                user::free_user_pml4(t.cr3);
            }
            t.state = State::Unused;
            let mut s = Serial::new(serial::COM1);
            let _ = writeln!(s, "sched: reaped tid {} (kernel stack + user pages freed)", tid);
        }
    }
}
