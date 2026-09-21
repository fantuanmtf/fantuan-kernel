//! Own syscall ABI v1 (DESIGN.md §4.6) + the P2 process/signal glue.
//!
//! Mechanism: INT 0x60, DPL 3 since M4.
//! Register ABI: rax = number, rdi..r8 = five arguments, rax = result.
//! Versioned ABI: SYS_VERSION probes ABI_VERSION and the call numbers live in
//! fantuan-abi; adding a call is an append-only change, never a renumbering.
//! Unknown numbers return ERR_NOSYS. v1 treats callers as trusted (no user
//! pointer validation beyond the page tables).
//!
//! P2: the arch publishes the live interrupt frame as `UserContext` through
//! kernel-core's FrameOps, so fork/execve/sigreturn can rewrite the ring-3
//! return context; `deliver_current` runs at every system-call exit.

use core::sync::atomic::{AtomicU64, Ordering};

use kernel_core::process::{FrameOps, UserContext};

use crate::interrupts::{user_frame, InterruptFrame};
use crate::serial;

pub const SYSCALL_VECTOR: u64 = 0x60;
pub const ABI_VERSION: u64 = kernel_core::syscall::ABI_VERSION;

// Numbers live in fantuan-abi — the ABI crate is the single source for both
// sides of the boundary.
pub use fantuan_abi::{
    SYS_EXIT, SYS_GET_TID, SYS_ERR_INVAL as ERR_INVAL, SYS_ERR_NOSYS as ERR_NOSYS,
    SYS_OK as OK, SYS_SIGRETURN, SYS_SLEEP_MS, SYS_VERSION, SYS_WRITE, SYS_YIELD,
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

/// Frame pointer published while a handler runs (single core, IRQs off).
static CUR_FRAME: AtomicU64 = AtomicU64::new(0);

/// Install the P2 FrameOps; call once at boot (after task::init_arch).
pub fn init_frame_ops() {
    kernel_core::process::set_frame_ops(FrameOps {
        get: frame_get,
        set: frame_set,
        load_root: frame_load_root,
    });
}

fn frame_get() -> Option<UserContext> {
    let p = CUR_FRAME.load(Ordering::Acquire);
    if p == 0 {
        return None;
    }
    let f = unsafe { &mut *(p as *mut InterruptFrame) };
    let (r15, r14, r13, r12, r11, r10, r9, r8) = (f.r15, f.r14, f.r13, f.r12, f.r11, f.r10, f.r9, f.r8);
    let (rbp, rdi, rsi, rdx, rcx, rbx, rax) = (f.rbp, f.rdi, f.rsi, f.rdx, f.rcx, f.rbx, f.rax);
    let uf = user_frame(f)?;
    Some(UserContext {
        r15, r14, r13, r12, r11, r10, r9, r8, rbp, rdi, rsi, rdx, rcx, rbx, rax,
        rip: uf.rip,
        rflags: uf.rflags,
        rsp: uf.rsp,
    })
}

fn frame_set(ctx: &UserContext) {
    let p = CUR_FRAME.load(Ordering::Acquire);
    if p == 0 {
        return;
    }
    let f = unsafe { &mut *(p as *mut InterruptFrame) };
    f.r15 = ctx.r15;
    f.r14 = ctx.r14;
    f.r13 = ctx.r13;
    f.r12 = ctx.r12;
    f.r11 = ctx.r11;
    f.r10 = ctx.r10;
    f.r9 = ctx.r9;
    f.r8 = ctx.r8;
    f.rbp = ctx.rbp;
    f.rdi = ctx.rdi;
    f.rsi = ctx.rsi;
    f.rdx = ctx.rdx;
    f.rcx = ctx.rcx;
    f.rbx = ctx.rbx;
    f.rax = ctx.rax;
    if let Some(uf) = user_frame(f) {
        uf.rip = ctx.rip;
        uf.rflags = ctx.rflags;
        uf.rsp = ctx.rsp;
    }
}

fn frame_load_root(root: u64) {
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) root, options(nostack, preserves_flags));
    }
    kernel_core::task::set_current_vm_root(root);
}

/// Deliver pending signals before the handler returns to ring 3.
pub fn deliver_user(frame: *mut InterruptFrame) {
    CUR_FRAME.store(frame as u64, Ordering::Release);
    kernel_core::signal::deliver_current();
    CUR_FRAME.store(0, Ordering::Release);
}

/// Called from isr_dispatch with the interrupt frame of INT 0x60.
pub fn dispatch(frame: &mut InterruptFrame) {
    let n = frame.rax;
    let args = [frame.rdi, frame.rsi, frame.rdx, frame.r10, frame.r8];
    CUR_FRAME.store(frame as *mut InterruptFrame as u64, Ordering::Release);
    let result = kernel_core::syscall::dispatch(sys_write, n, &args);
    if n != SYS_SIGRETURN && n != fantuan_abi::SYS_EXECVE {
        // SIGRETURN/EXECVE replace the whole ring-3 context already; writing
        // the syscall result would clobber RAX = the restored/entry value.
        frame.rax = result;
    }
    // The dispatch may have slept (wait4/pipe/tty/nanosleep) and another task
    // may have used the global meanwhile: re-publish this frame before the
    // signal delivery reads/writes the ring-3 context.
    CUR_FRAME.store(frame as *mut InterruptFrame as u64, Ordering::Release);
    kernel_core::signal::deliver_current();
    CUR_FRAME.store(0, Ordering::Release);
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
