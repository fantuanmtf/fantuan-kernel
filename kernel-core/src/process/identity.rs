//! Task identity and the exit/reap bookkeeping: which slot is which id, who
//! is whose parent, how a finished task's status is recorded, and how it is
//! collected. A child of `process`; see `super` for the table and locking
//! model.

use super::{lock, procs, EMPTY_PROC, MAX_TASKS};
use super::sigstate::sig_mask;

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

pub(super) fn slot_of_id(id: u64) -> Option<usize> {
    (0..MAX_TASKS).find(|&s| procs()[s].used && procs()[s].id == id)
}

pub fn id_of(slot: usize) -> u64 {
    let _g = lock();
    procs()[slot].id
}
