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

pub const SYSCALL_VECTOR: u64 = 0x60;
pub const ABI_VERSION: u64 = kernel_core::syscall::ABI_VERSION;

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

/// Install the P1 user-memory bridge (docs/POSIX_PLAN.md). Under SMAP the
/// kernel cannot touch user pages without AC, so every copy is bracketed by
/// stac/clac; addresses in the kernel half are refused outright.
pub fn init_mem_ops() {
    kernel_core::user::set_mem_ops(kernel_core::user::UserMemOps { copy_in, copy_out });
}

fn copy_in(dst: &mut [u8], src: u64) -> bool {
    let Some(end) = src.checked_add(dst.len() as u64) else { return false };
    if src == 0 || end > fantuan_abi::PHYS_OFFSET {
        return false;
    }
    crate::cpu::stac();
    unsafe { core::ptr::copy_nonoverlapping(src as *const u8, dst.as_mut_ptr(), dst.len()) };
    crate::cpu::clac();
    true
}

fn copy_out(dst: u64, src: &[u8]) -> bool {
    let Some(end) = dst.checked_add(src.len() as u64) else { return false };
    if dst == 0 || end > fantuan_abi::PHYS_OFFSET {
        return false;
    }
    crate::cpu::stac();
    unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst as *mut u8, src.len()) };
    crate::cpu::clac();
    true
}

/// Called from isr_dispatch with the interrupt frame of INT 0x60.
pub fn dispatch(frame: &mut InterruptFrame) {
    let args = [frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8];
    frame.rax = kernel_core::syscall::dispatch(sys_write, frame.rax, &args);
}

fn sys_write(ptr: u64, len: u64) -> u64 {
    let len = len.min(512);
    if ptr == 0 {
        return ERR_INVAL;
    }
    // User pointers are validated by the page tables (a bad address faults
    // and kills the task). With SMAP enabled (M8.3c) supervisor access to
    // user pages requires the AC flag: stac/clac bracket the copy.
    crate::cpu::stac();
    let buf = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let _ = serial::write_locked(buf);
    crate::cpu::clac();
    OK
}
