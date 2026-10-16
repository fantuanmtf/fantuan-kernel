//! P2 process layer (docs/POSIX_PLAN.md): fork/execve/wait4, process groups,
//! sessions and the per-task signal state. Static tables keyed by task slot
//! (no kernel heap), serialized by `PROC_LOCK`; functions that block release
//! the lock first.
//!
//! Address-space mechanics stay behind UserOps (clone_root/unmap/protect) and
//! TaskOps (init_fork_stack/exec_image); the arch glue implements those, so
//! this file is the policy and the tables. Kernels that never install the
//! P2 ops report ERR_NOSYS for fork/exec.

use core::sync::atomic::AtomicBool;

use fantuan_abi::{NSIG, SIG_DFL};

use crate::arch::IrqLock;
use crate::task;

pub const MAX_TASKS: usize = task::MAX_TASKS;
pub const MAX_ARGS: usize = 16;
pub const ARG_MAX: usize = 128;

static PROC_LOCK: AtomicBool = AtomicBool::new(false);

fn lock() -> IrqLock {
    IrqLock::acquire(&PROC_LOCK)
}

/// The user register context the arch snapshots at a syscall/interrupt
/// boundary and writes back before returning to ring 3.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct UserContext {
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
    pub rip: u64,
    pub rflags: u64,
    pub rsp: u64,
}

/// Installed by an arch with user mode (x86_64 P2); all-default otherwise.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct FrameOps {
    /// Snapshot the ring-3 context; None when the interruption was ring 0.
    pub get: fn() -> Option<UserContext>,
    /// Replace the return context before the handler returns to ring 3.
    pub set: fn(&UserContext),
    /// Switch the live CR3 to a new user root (execve).
    pub load_root: fn(u64),
}

fn unset_get() -> Option<UserContext> {
    None
}
fn unset_set(_c: &UserContext) {}
fn unset_root(_r: u64) {}

static mut FRAME_OPS: FrameOps = FrameOps { get: unset_get, set: unset_set, load_root: unset_root };

pub fn set_frame_ops(ops: FrameOps) {
    unsafe { core::ptr::write(core::ptr::addr_of_mut!(FRAME_OPS), ops) };
}

pub(crate) fn frame_ops() -> FrameOps {
    unsafe { core::ptr::addr_of!(FRAME_OPS).read() }
}

/// Static ELF registry the kernel installs (embedded /bin/dash, ...); execve
/// checks it before any filesystem path.
static mut BIN_LOOKUP: Option<fn(&[u8]) -> Option<&'static [u8]>> = None;

pub fn set_bin_lookup(f: fn(&[u8]) -> Option<&'static [u8]>) {
    unsafe { core::ptr::write(core::ptr::addr_of_mut!(BIN_LOOKUP), Some(f)) };
}

/// Resolve a registered binary path (used by execve and the VFS registry
/// that exposes /bin entries to stat/open without an on-disk file).
pub fn lookup_bin(path: &[u8]) -> Option<&'static [u8]> {
    unsafe { core::ptr::addr_of!(BIN_LOOKUP).read()? (path) }
}

#[derive(Clone, Copy, Default)]
pub struct Action {
    pub handler: u64,
    pub mask: u64,
    pub flags: i32,
    pub restorer: u64,
}

#[derive(Clone, Copy, Default)]
pub struct Vma {
    pub start: u64,
    pub end: u64,
    pub prot: u64,
}

#[derive(Clone, Copy)]
pub struct Proc {
    pub used: bool,
    pub alive: bool,
    pub id: u64,
    pub ppid: u64,
    pub pgid: u64,
    pub sid: u64,
    pub exit_status: u64,
    pub waited: bool,
    pub pending: u32,
    pub blocked: u32,
    pub actions: [Action; NSIG as usize],
    pub vmas: [Vma; fantuan_abi::MAX_VMAS],
    pub vma_n: u8,
    pub mmap_next: u64,
}

const EMPTY_PROC: Proc = Proc {
    used: false,
    alive: false,
    id: 0,
    ppid: 0,
    pgid: 0,
    sid: 0,
    exit_status: 0,
    waited: false,
    pending: 0,
    blocked: 0,
    actions: [Action { handler: SIG_DFL, mask: 0, flags: 0, restorer: 0 }; NSIG as usize],
    vmas: [Vma { start: 0, end: 0, prot: 0 }; fantuan_abi::MAX_VMAS],
    vma_n: 0,
    mmap_next: fantuan_abi::USER_MMAP_BASE,
};

static mut PROCS: [Proc; MAX_TASKS] = [EMPTY_PROC; MAX_TASKS];
static mut TTY_PGRP: u64 = 0;

fn procs() -> &'static mut [Proc; MAX_TASKS] {
    unsafe { &mut *core::ptr::addr_of_mut!(PROCS) }
}

// Split by responsibility in 2026-10, when this file stood at 911 lines and
// the budget entry said "split by responsibility before M14-1 touches it".
// `Proc` stays whole here: it is one static record per task slot (there is no
// kernel heap), so the seam is by function over shared state, not by field.
// The children reach `procs()`, `lock()` and `TTY_PGRP` through `super`,
// which child modules may do for private items - that is what keeps the
// tables single-sourced instead of copied.
//
// M14-1 (kernel heap, demand paging, COW) lands in `vma`, which is the point
// of the split: it should not have to touch this file.

pub mod group;
pub mod identity;
pub mod lifecycle;
pub mod sigstate;
pub mod vma;

pub use group::{getpgid, getpgrp, getppid, setpgid, set_tty_pgrp, setsid, signal_console, tty_pgrp};
pub use identity::{
    alive_by_id, clone_task, id_of, init_boot, init_task, on_exit, on_reap, reapable,
    status_exited, status_signaled,
};
pub use lifecycle::{execve, fork, kill, wait4};
pub use sigstate::{
    action_of, block_for_handler, blocked_of, default_ignored, dequeue, has_deliverable,
    pending_handled, queue_slot, set_action, set_blocked, sigaction_get, sigaction_set,
    sigprocmask,
};
pub use vma::{mmap2, mprotect, munmap, vma_clear, vma_contains, vma_forget, vma_reprotect};
