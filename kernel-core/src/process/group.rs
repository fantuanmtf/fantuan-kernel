//! Process groups, sessions, and the controlling terminal's foreground
//! group. A child of `process`; see `super` for the table and locking model.

use super::{lock, procs, MAX_TASKS, TTY_PGRP};
use super::identity::slot_of_id;
use super::lifecycle::kill;
use super::sigstate::queue_slot;
use fantuan_abi::{SYS_ERR_SRCH, SYS_OK};
use crate::task;

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
