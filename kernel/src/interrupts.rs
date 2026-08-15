//! ISR dispatch: every interrupt lands here (from interrupts.S), routed to
//! the exception handlers or the IRQ handlers.

use core::fmt::Write;
use core::mem::size_of;

use crate::consts::{IRQ_TIMER, PIC2_OFFSET};
use crate::exceptions;
use crate::pic;
use crate::serial::{self, Serial};
use crate::syscall;
use crate::timer;

/// Saved by isr_common in exactly this order (interrupts.S).
#[repr(C)]
pub struct InterruptFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub vector: u64,
    pub error_code: u64,
}

/// The CPU-pushed frame for a kernel-mode interrupt (no SS/RSP: same ring).
#[repr(C)]
pub(crate) struct CpuFrame {
    pub(crate) rip: u64,
    pub(crate) cs: u64,
    pub(crate) rflags: u64,
}

pub fn cpu_frame(frame: &InterruptFrame) -> &CpuFrame {
    unsafe {
        &*((frame as *const InterruptFrame as *const u8)
            .add(size_of::<InterruptFrame>()) as *const CpuFrame)
    }
}

/// Called from isr_common with rdi = *mut InterruptFrame.
#[no_mangle]
pub extern "C" fn isr_dispatch(frame: *mut InterruptFrame) {
    let f = unsafe { &mut *frame };
    let vector = f.vector;

    if vector == syscall::SYSCALL_VECTOR {
        syscall::dispatch(f);
    } else if vector < 32 {
        exceptions::handle(f);
    } else if vector >= 32 && vector < 48 {
        pic::eoi(vector as u8);
        if vector == IRQ_TIMER as u64 {
            timer::tick();
        }
    } else {
        let mut s = Serial::new(serial::COM1);
        let _ = writeln!(s, "irq: unexpected vector {} (>={})", vector, PIC2_OFFSET);
    }
}
