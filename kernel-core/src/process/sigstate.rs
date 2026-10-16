//! Per-slot signal state: the disposition table and the pending/blocked
//! masks. `signal` delivers against what this holds. A child of `process`;
//! see `super` for the table and locking model.

use super::{lock, procs, Action};
use fantuan_abi::{NSIG, SIGKILL, SIG_BLOCK, SIG_SETMASK, SIG_UNBLOCK, SYS_ERR_INVAL, SYS_OK};

pub(super) fn sig_mask(sig: u32) -> u32 {
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
