//! Task lifecycle: the operations that create, wait for, replace and end a
//! task - fork, execve, wait4 and kill. A child of `process`; see `super`
//! for the table and locking model.

use super::{frame_ops, lock, lookup_bin, procs, Action, UserContext, ARG_MAX, MAX_ARGS, MAX_TASKS};
use super::sigstate::{pending_handled, queue_slot};
use super::vma::vma_clear;
use fantuan_abi::{
    NSIG, SIG_DFL, SIG_IGN, SYS_ERR_CHILD, SYS_ERR_FAULT, SYS_ERR_INTR, SYS_ERR_INVAL,
    SYS_ERR_NOEXEC, SYS_ERR_NOENT, SYS_ERR_NOMEM, SYS_ERR_NOSYS, SYS_ERR_SRCH, SYS_OK, WNOHANG,
};
use crate::task;
use crate::user;

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
