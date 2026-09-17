//! Own syscall ABI v1 (DESIGN.md §4.6).
//!
//! Mechanism: INT 0x60, DPL 3 since M4.
//! Register ABI: rax = number, rdi..r8 = five arguments, rax = result.
//! Versioned ABI: SYS_VERSION probes ABI_VERSION and the call numbers live in
//! fantuan-abi; adding a call is an append-only change, never a renumbering.
//! Unknown numbers return ERR_NOSYS. v1 treats callers as trusted (no user
//! pointer validation beyond the page tables).

use crate::interrupts::InterruptFrame;
use crate::serial;
use crate::task;

pub const SYSCALL_VECTOR: u64 = 0x60;
pub const ABI_VERSION: u64 = 1;

// Numbers live in fantuan-abi — the ABI crate is the single source for both
// sides of the boundary.
pub use fantuan_abi::{
    SYS_EXIT, SYS_GET_TID, SYS_ERR_INVAL as ERR_INVAL, SYS_ERR_NOSYS as ERR_NOSYS,
    SYS_OK as OK, SYS_SLEEP_MS, SYS_VERSION, SYS_WRITE, SYS_YIELD,
};

extern "C" {
    fn syscall_trampoline(n: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64;
}

/// Call the kernel. The trampoline reserves scratch below rsp so the interrupt
/// frame cannot touch the caller's red zone.
#[inline]
pub fn syscall(n: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64 {
    unsafe { syscall_trampoline(n, a1, a2, a3, a4, a5) }
}

/// Called from isr_dispatch with the interrupt frame of INT 0x60.
pub fn dispatch(frame: &mut InterruptFrame) {
    let n = frame.rax;
    let result = match n {
        SYS_VERSION => ABI_VERSION,
        SYS_GET_TID => task::current_id(),
        SYS_EXIT => task::exit(frame.rdi),
        SYS_SLEEP_MS => {
            task::sleep_ms(frame.rdi);
            OK
        }
        SYS_WRITE => sys_write(frame.rdi, frame.rsi),
        SYS_YIELD => {
            task::schedule();
            OK
        }
        _ => ERR_NOSYS,
    };
    frame.rax = result;
}

fn sys_write(ptr: u64, len: u64) -> u64 {
    let len = len.min(512);
    if ptr == 0 {
        return ERR_INVAL;
    }
    // M4: user pointers are validated by the page tables (a bad address
    // faults and kills the task); SMAP/SMEP hardening arrives later.
    let buf = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let _ = serial::write_locked(buf);
    OK
}
