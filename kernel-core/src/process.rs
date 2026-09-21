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

use fantuan_abi::{
    MAP_ANONYMOUS, MAP_FIXED, MAP_SHARED, NSIG, SIGKILL, SIG_BLOCK, SIG_DFL, SIG_IGN, SIG_SETMASK,
    SIG_UNBLOCK, SYS_ERR_CHILD, SYS_ERR_FAULT, SYS_ERR_INTR, SYS_ERR_INVAL, SYS_ERR_NOEXEC,
    SYS_ERR_NOENT, SYS_ERR_NOMEM, SYS_ERR_NOSYS, SYS_ERR_SRCH, SYS_OK, WNOHANG,
};

use crate::arch::IrqLock;
use crate::task;
use crate::user;

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

/// Boot/idle "maintenance" process owns slot 0 so the kernel shell can spawn
/// and wait for children (ppid 0) before any user task exists.
pub fn init_boot() {
    let p = &mut procs()[0];
    p.used = true;
    p.alive = true;
    p.id = 0;
    p.pgid = 0;
    p.sid = 0;
}

/// Fresh process for a newly registered user task.
pub fn init_task(slot: usize, id: u64, ppid: u64) {
    let _g = lock();
    let p = &mut procs()[slot];
    *p = EMPTY_PROC;
    p.used = true;
    p.alive = true;
    p.id = id;
    p.ppid = ppid;
    p.pgid = if ppid == 0 { id } else { procs()[slot_of_id(ppid).unwrap_or(0)].pgid };
    p.sid = if ppid == 0 { id } else { procs()[slot_of_id(ppid).unwrap_or(0)].sid };
}

/// Fork: copy the non-pending signal/VMA state of the parent.
pub fn clone_task(parent_slot: usize, child_slot: usize, id: u64) {
    let _g = lock();
    let parent = procs()[parent_slot];
    let mut p = parent;
    p.id = id;
    p.ppid = parent.id;
    p.alive = true;
    p.waited = false;
    p.exit_status = 0;
    p.pending = 0;
    procs()[child_slot] = p;
}

/// Encode an exit (POSIX wait-status convention: exit code in bits 15:8).
pub fn status_exited(code: u64) -> u64 {
    (code & 0xff) << 8
}

/// Encode a signal death (low 7 bits).
pub fn status_signaled(sig: u32) -> u64 {
    sig as u64
}

/// Child exit: publish the zombie status and notify the parent.
pub fn on_exit(slot: usize, status: u64) {
    // No lock: task::exit may be called from the signal layer holding it.
    let flags = crate::arch::irq_save();
    let p = &mut procs()[slot];
    let parent = p.ppid;
    let used = p.used;
    if used {
        p.alive = false;
        p.exit_status = status;
    }
    if used && parent != 0 {
        if let Some(ps) = slot_of_id(parent) {
            procs()[ps].pending |= sig_mask(fantuan_abi::SIGCHLD);
        }
    }
    crate::arch::irq_restore(flags);
}

/// A slot may be freed once it is a waited zombie or its parent is gone.
pub fn reapable(slot: usize) -> bool {
    let p = procs()[slot];
    if !p.used || p.alive {
        return false;
    }
    p.waited || p.ppid == 0 || slot_of_id(p.ppid).is_none()
}

pub fn on_reap(slot: usize) {
    let _g = lock();
    procs()[slot] = EMPTY_PROC;
}

pub fn alive_by_id(id: u64) -> bool {
    let _g = lock();
    slot_of_id(id).map(|s| procs()[s].alive).unwrap_or(false)
}

fn slot_of_id(id: u64) -> Option<usize> {
    (0..MAX_TASKS).find(|&s| procs()[s].used && procs()[s].id == id)
}

pub fn id_of(slot: usize) -> u64 {
    let _g = lock();
    procs()[slot].id
}

fn sig_mask(sig: u32) -> u32 {
    if sig < NSIG {
        1u32 << sig
    } else {
        0
    }
}

// --- signal state (query/set helpers used by the syscall layer) ------------

pub fn sigaction_set(slot: usize, sig: u32, act: &fantuan_abi::SigAction) -> u64 {
    if sig == 0 || sig >= NSIG || sig == SIGKILL {
        return SYS_ERR_INVAL;
    }
    let _g = lock();
    procs()[slot].actions[sig as usize] = Action {
        handler: act.sa_handler,
        mask: act.sa_mask,
        flags: act.sa_flags,
        restorer: act.sa_restorer,
    };
    SYS_OK
}

pub fn sigaction_get(slot: usize, sig: u32) -> Option<fantuan_abi::SigAction> {
    if sig == 0 || sig >= NSIG || sig == SIGKILL {
        return None;
    }
    let _g = lock();
    let a = procs()[slot].actions[sig as usize];
    Some(fantuan_abi::SigAction {
        sa_handler: a.handler,
        sa_mask: a.mask,
        sa_flags: a.flags,
        _pad: 0,
        sa_restorer: a.restorer,
    })
}

pub fn sigprocmask(slot: usize, how: u64, set: u32) -> u64 {
    let _g = lock();
    let p = &mut procs()[slot];
    match how {
        SIG_BLOCK => p.blocked |= set,
        SIG_UNBLOCK => p.blocked &= !set,
        SIG_SETMASK => p.blocked = set,
        _ => return SYS_ERR_INVAL,
    }
    p.blocked &= !sig_mask(SIGKILL);
    SYS_OK
}

pub fn blocked_of(slot: usize) -> u32 {
    let _g = lock();
    procs()[slot].blocked
}

pub fn set_blocked(slot: usize, mask: u32) {
    let _g = lock();
    procs()[slot].blocked = mask & !sig_mask(SIGKILL);
}

/// Queue a signal for a slot (SIGKILL always wins and cannot be blocked).
pub fn queue_slot(slot: usize, sig: u32) {
    if sig == 0 || sig >= NSIG {
        return;
    }
    let flags = crate::arch::irq_save();
    procs()[slot].pending |= sig_mask(sig);
    crate::arch::irq_restore(flags);
}

/// Lowest deliverable pending signal; SIGKILL bypasses the blocked mask.
pub fn dequeue(slot: usize) -> Option<u32> {
    let _g = lock();
    let p = &mut procs()[slot];
    let kill = sig_mask(SIGKILL);
    if p.pending & kill != 0 {
        p.pending &= !kill;
        return Some(SIGKILL);
    }
    let ready = p.pending & !p.blocked;
    if ready == 0 {
        return None;
    }
    let sig = ready.trailing_zeros();
    p.pending &= !(1u32 << sig);
    Some(sig)
}

/// A handled signal waiting while blocked in a syscall (wait4 EINTR).
pub fn pending_handled(slot: usize) -> bool {
    let _g = lock();
    let p = procs()[slot];
    let ready = p.pending & !p.blocked;
    (1..NSIG).any(|s| ready & (1 << s) != 0 && !default_ignored(s))
}

pub fn action_of(slot: usize, sig: u32) -> Action {
    let _g = lock();
    procs()[slot].actions[sig as usize]
}

pub fn set_action(slot: usize, sig: u32, a: Action) {
    let _g = lock();
    procs()[slot].actions[sig as usize] = a;
}

/// Entering a handler: block the signal and its sa_mask.
pub fn block_for_handler(slot: usize, sig: u32, mask: u64) {
    let _g = lock();
    let p = &mut procs()[slot];
    p.blocked |= sig_mask(sig) | (mask as u32);
    p.blocked &= !sig_mask(SIGKILL);
}

/// Is any signal deliverable right now (including blocked SIGKILL)?
pub fn has_deliverable(slot: usize) -> bool {
    let _g = lock();
    let p = procs()[slot];
    p.pending & (!p.blocked | sig_mask(SIGKILL)) != 0
}

/// Signals whose default disposition is to ignore (POSIX).
pub fn default_ignored(sig: u32) -> bool {
    matches!(
        sig,
        fantuan_abi::SIGCHLD
            | fantuan_abi::SIGCONT
            | 23 // SIGURG
            | fantuan_abi::SIGWINCH
            | fantuan_abi::SIGSTOP
            | fantuan_abi::SIGTSTP
            | fantuan_abi::SIGTTIN
            | fantuan_abi::SIGTTOU
    )
}

// --- process groups / sessions ---------------------------------------------

pub fn getpgrp() -> u64 {
    let _g = lock();
    procs()[task::current_slot()].pgid
}

pub fn getpgid(pid: u64) -> Result<u64, u64> {
    let _g = lock();
    let s = if pid == 0 { task::current_slot() } else { slot_of_id(pid).ok_or(SYS_ERR_SRCH)? };
    Ok(procs()[s].pgid)
}

pub fn getppid() -> u64 {
    let _g = lock();
    procs()[task::current_slot()].ppid
}

pub fn setpgid(pid: u64, pgid: u64) -> u64 {
    let _g = lock();
    let cur = task::current_slot();
    let s = if pid == 0 { cur } else { match slot_of_id(pid) { Some(s) => s, None => return SYS_ERR_SRCH } };
    if s != cur && procs()[s].ppid != procs()[cur].id {
        return SYS_ERR_SRCH; // only your own children
    }
    let pgid = if pgid == 0 { procs()[s].id } else { pgid };
    if pgid != procs()[s].id && slot_pgrp_exists(pgid).is_none() {
        return SYS_ERR_SRCH;
    }
    procs()[s].pgid = pgid;
    SYS_OK
}

fn slot_pgrp_exists(pgid: u64) -> Option<usize> {
    (0..MAX_TASKS).find(|&s| procs()[s].used && procs()[s].pgid == pgid)
}

pub fn setsid() -> u64 {
    let _g = lock();
    let cur = task::current_slot();
    let id = procs()[cur].id;
    procs()[cur].sid = id;
    procs()[cur].pgid = id;
    id
}

pub fn tty_pgrp() -> u64 {
    let _g = lock();
    unsafe { *core::ptr::addr_of!(TTY_PGRP) }
}

pub fn set_tty_pgrp(pgrp: u64) {
    let _g = lock();
    unsafe { *core::ptr::addr_of_mut!(TTY_PGRP) = pgrp };
}

/// SIGINT from the console: the terminal's foreground group, else the current
/// task's group (the kernel shell before tcsetpgrp runs).
pub fn signal_console(sig: u32) {
    let pgrp = tty_pgrp();
    if pgrp != 0 {
        kill(-(pgrp as i64), sig);
    } else {
        let cur = task::current_slot();
        queue_slot(cur, sig);
    }
}

// --- kill -------------------------------------------------------------------

pub fn kill(pid: i64, sig: u32) -> u64 {
    if sig >= NSIG {
        return SYS_ERR_INVAL;
    }
    let cur = task::current_slot();
    let cur_id;
    let cur_pgid;
    {
        let _g = lock();
        cur_id = procs()[cur].id;
        cur_pgid = procs()[cur].pgid;
    }
    let mut victims = [false; MAX_TASKS];
    let mut any = false;
    {
        let _g = lock();
        for s in 0..MAX_TASKS {
            let p = procs()[s];
            if !p.used || !p.alive {
                continue;
            }
            let hit = match pid {
                0 => p.pgid == cur_pgid,
                -1 => p.id != 0 && p.id != cur_id,
                n if n > 0 => p.id == n as u64,
                n => p.pgid == (-n) as u64,
            };
            if hit {
                victims[s] = true;
                any = true;
            }
        }
    }
    if !any {
        return SYS_ERR_SRCH;
    }
    if sig == 0 {
        return SYS_OK;
    }
    for s in 0..MAX_TASKS {
        if victims[s] {
            queue_slot(s, sig);
        }
    }
    SYS_OK
}

// --- wait4 ------------------------------------------------------------------

/// Blocking wait (WNOHANG honored); returns (pid, status). Sleeping happens
/// outside the lock so the children can run.
pub fn wait4(pid: i64, options: u64) -> Result<(i64, u64), u64> {
    let cur = task::current_slot();
    let (my_id, my_pgid);
    {
        let _g = lock();
        my_id = procs()[cur].id;
        my_pgid = procs()[cur].pgid;
    }
    loop {
        let found = {
            let _g = lock();
            (0..MAX_TASKS).find_map(|s| {
                let p = procs()[s];
                if !p.used || p.alive || p.waited || p.ppid != my_id {
                    return None;
                }
                let hit = if pid > 0 {
                    p.id == pid as u64
                } else if pid == 0 {
                    p.pgid == my_pgid
                } else if pid == -1 {
                    true
                } else {
                    p.pgid == (-pid) as u64
                };
                if hit {
                    Some((s, p.id, p.exit_status))
                } else {
                    None
                }
            })
        };
        if let Some((slot, child, status)) = found {
            let _g = lock();
            procs()[slot].waited = true;
            return Ok((child as i64, status));
        }
        if options & WNOHANG != 0 {
            return Ok((0, 0));
        }
        let children = {
            let _g = lock();
            (0..MAX_TASKS).any(|s| procs()[s].used && procs()[s].ppid == my_id)
        };
        if !children {
            return Err(SYS_ERR_CHILD);
        }
        if pending_handled(cur) {
            return Err(SYS_ERR_INTR);
        }
        task::sleep_ms(1);
    }
}

// --- fork / exec ------------------------------------------------------------

/// Parse a user argv/envp vector into fixed kernel buffers.
fn parse_vec(mut ptr: u64) -> Result<([[u8; ARG_MAX]; MAX_ARGS], usize), u64> {
    let mut out = [[0u8; ARG_MAX]; MAX_ARGS];
    let mut n = 0usize;
    if ptr == 0 {
        return Ok((out, 0));
    }
    loop {
        if n >= MAX_ARGS {
            return Err(SYS_ERR_INVAL);
        }
        let mut w = [0u8; 8];
        if user::copy_in(&mut w, ptr).is_none() {
            return Err(SYS_ERR_FAULT);
        }
        let p = u64::from_le_bytes(w);
        if p == 0 {
            break;
        }
        let mut s = [0u8; ARG_MAX];
        for (i, b) in s.iter_mut().enumerate() {
            let mut one = [0u8; 1];
            if user::copy_in(&mut one, p + i as u64).is_none() {
                return Err(SYS_ERR_FAULT);
            }
            *b = one[0];
            if one[0] == 0 {
                break;
            }
            if i == ARG_MAX - 1 {
                return Err(SYS_ERR_INVAL); // too long
            }
        }
        out[n] = s;
        n += 1;
        ptr += 8;
    }
    Ok((out, n))
}

/// fork(): duplicate the caller's address space and state. Returns the child
/// pid in the parent; the child resumes here with RAX = 0 (arch stack build).
pub fn fork() -> u64 {
    let ops = frame_ops();
    let Some(ctx) = (ops.get)() else { return SYS_ERR_NOSYS };
    let task_ops = task::ops();
    let parent = task::current_slot();
    let parent_root = task::current_vm_root();
    let Some(child_root) = (user::ops().clone_root)(parent_root) else {
        return SYS_ERR_NOMEM;
    };
    let child_ctx = UserContext { rax: 0, ..ctx };
    let Some((stack_phys, stack_top, rsp)) = (task_ops.init_fork_stack)(&child_ctx) else {
        (user::ops().free_root)(child_root);
        return SYS_ERR_NOMEM;
    };
    let t = task::Task {
        state: task::State::Ready,
        rsp,
        vm_root: child_root,
        kernel_stack_top: stack_top,
        is_user: true,
        stack_phys,
        body: task::dead_body,
        id: 0,
        exit_code: 0,
        heap_base: 0,
    };
    match task::register_fork(parent, t) {
        Some(id) => id,
        None => {
            (user::ops().free_root)(child_root);
            SYS_ERR_NOMEM
        }
    }
}

/// execve(path, argv, envp): replace the image, keep the pid, keep fds
/// (CLOEXEC ones are closed), reset caught handlers and the VMA/brk state.
pub fn execve(path: u64, argv: u64, envp: u64) -> u64 {
    if (frame_ops().get)().is_none() {
        return SYS_ERR_NOSYS;
    }
    let (pbuf, plen) = match crate::vfs::posix::cpath(path) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let path_slice = crate::vfs::posix::cpath_slice(&pbuf, plen);
    let (argv_buf, argc) = match parse_vec(argv) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let (env_buf, envc) = match parse_vec(envp) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let Some(image) = lookup_bin(path_slice) else {
        return SYS_ERR_NOENT;
    };
    let mut arg_refs = [&[][..]; MAX_ARGS];
    for i in 0..argc {
        let n = argv_buf[i].iter().position(|&b| b == 0).unwrap_or(ARG_MAX);
        arg_refs[i] = &argv_buf[i][..n];
    }
    const DEFAULT_ENV: [&[u8]; 5] = [
        b"PATH=/bin:/usr/bin",
        b"HOME=/",
        b"PWD=/",
        b"TERM=fantuan",
        b"USER=root",
    ];
    let mut env_refs = [&[][..]; MAX_ARGS];
    let mut envc_used = envc;
    for i in 0..envc {
        let n = env_buf[i].iter().position(|&b| b == 0).unwrap_or(ARG_MAX);
        env_refs[i] = &env_buf[i][..n];
    }
    if envc_used == 0 {
        envc_used = DEFAULT_ENV.len();
        env_refs[..envc_used].copy_from_slice(&DEFAULT_ENV);
    }
    let task_ops = task::ops();
    let Some((entry, user_rsp, new_root, heap_base)) =
        (task_ops.exec_image)(image, &arg_refs[..argc.max(1)], &env_refs[..envc_used])
    else {
        return SYS_ERR_NOEXEC;
    };

    let slot = task::current_slot();
    let old_root = task::current_vm_root();
    // Reset process signal context: caught handlers to default, ignored kept.
    signal_reset_for_exec(slot);
    crate::vfs::fd::exec_close(slot);
    crate::brk::init_task(slot, heap_base);
    vma_clear(slot);
    task::set_current_vm_root(new_root);
    (frame_ops().load_root)(new_root);
    (user::ops().free_root)(old_root);
    (frame_ops().set)(&UserContext { rip: entry, rsp: user_rsp, ..Default::default() });
    SYS_OK
}

fn signal_reset_for_exec(slot: usize) {
    let _g = lock();
    let p = &mut procs()[slot];
    for s in 1..NSIG as usize {
        if p.actions[s].handler != SIG_IGN {
            p.actions[s] = Action { handler: SIG_DFL, mask: 0, flags: 0, restorer: 0 };
        }
    }
    p.pending = 0;
}

// --- VMA / mmap --------------------------------------------------------------

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
