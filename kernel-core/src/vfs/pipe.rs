//! Pipe buffers for the P1 POSIX layer: 8 fixed pipes of 512 bytes.
//!
//! A pipe is a byte buffer (the shift-down implementation is fine at 512 B)
//! with separate read/write end refcounts; the last writer closing makes
//! `read` return EOF and the last reader closing makes `write` fail EPIPE.
//! `read`/`write` acquire the shared FS lock for each probe and deliberately
//! drop it across `task::sleep_ms`, so the peer task can make progress.
//! `alloc`/`init`/`drop_end`/`unread` run with the caller holding the lock.

use fantuan_abi::{SYS_ERR_BADF, SYS_ERR_INTR, SYS_ERR_PIPE};

use super::fd::FS_LOCK;
use super::ERR_WOULD_BLOCK;
use crate::task;

const MAX_PIPES: usize = 8;
const PIPE_BUF: usize = 512;

#[derive(Clone, Copy)]
struct Pipe {
    used: bool,
    len: u16,
    read_refs: u8,
    write_refs: u8,
    buf: [u8; PIPE_BUF],
}

const EMPTY_PIPE: Pipe = Pipe { used: false, len: 0, read_refs: 0, write_refs: 0, buf: [0; PIPE_BUF] };

static mut PIPES: [Pipe; MAX_PIPES] = [EMPTY_PIPE; MAX_PIPES];

fn pipes() -> &'static mut [Pipe; MAX_PIPES] {
    unsafe { &mut *core::ptr::addr_of_mut!(PIPES) }
}

/// Allocate a pipe id; the caller holds the FS lock.
pub(super) fn alloc() -> Option<u16> {
    (0..MAX_PIPES).find(|&i| !pipes()[i].used).map(|i| i as u16)
}

/// Initialize a reserved pipe with both ends open; caller holds the lock.
pub(super) fn init(id: u16) {
    pipes()[id as usize] = EMPTY_PIPE;
    pipes()[id as usize].used = true;
    pipes()[id as usize].read_refs = 1;
    pipes()[id as usize].write_refs = 1;
}

/// Drop one reference to an end; frees the pipe when both refcounts are 0.
/// Caller holds the lock.
pub(super) fn drop_end(id: u16, write_end: bool) {
    let p = &mut pipes()[id as usize];
    if !p.used {
        return;
    }
    if write_end {
        p.write_refs = p.write_refs.saturating_sub(1);
    } else {
        p.read_refs = p.read_refs.saturating_sub(1);
    }
    if p.read_refs == 0 && p.write_refs == 0 {
        pipes()[id as usize] = EMPTY_PIPE;
    }
}

/// Unread bytes (fstat); caller holds the lock.
pub(super) fn unread(id: u16) -> u64 {
    pipes()[id as usize].len as u64
}

fn plog2(tag: char, id: usize, a: u64, b: u64) {
    use core::fmt::Write;
    struct S;
    impl Write for S {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            crate::log::put(s.as_bytes());
            Ok(())
        }
    }
    let _ = write!(S, "pipe {} id={} ret={} slot={}\n", tag, id, a, b);
}

fn read_locked(id: usize, out: &mut [u8]) -> Result<usize, u64> {
    let p = pipes()[id];
    if !p.used {
        return Err(SYS_ERR_BADF);
    }
    if p.len > 0 {
        let take = (p.len as usize).min(out.len());
        out[..take].copy_from_slice(&p.buf[..take]);
        let rest = p.len as usize - take;
        for i in 0..rest {
            pipes()[id].buf[i] = pipes()[id].buf[take + i];
        }
        pipes()[id].len = rest as u16;
        return Ok(take);
    }
    if p.write_refs == 0 || out.is_empty() {
        return Ok(0); // EOF (all writers closed) or a zero-length read
    }
    Err(ERR_WOULD_BLOCK)
}

/// Blocking pipe read (drops the FS lock across sleeps).
pub fn read(id: u16, out: &mut [u8]) -> Result<usize, u64> {
    loop {
        let r = {
            let _g = crate::arch::IrqLock::acquire(&FS_LOCK);
            read_locked(id as usize, out)
        };
        match r {
            Err(e) if e == ERR_WOULD_BLOCK => {
                if crate::process::pending_handled(task::current_slot()) {
                    return Err(SYS_ERR_INTR);
                }
                task::sleep_ms(1);
            }
            other => return other,
        }
    }
}

fn write_locked(id: usize, data: &[u8]) -> Result<usize, u64> {
    let p = pipes()[id];
    if !p.used {
        return Err(SYS_ERR_BADF);
    }
    if p.read_refs == 0 {
        return Err(SYS_ERR_PIPE);
    }
    let space = PIPE_BUF - p.len as usize;
    if space > 0 {
        let take = space.min(data.len()).min(PIPE_BUF);
        let base = p.len as usize;
        pipes()[id].buf[base..base + take].copy_from_slice(&data[..take]);
        pipes()[id].len = (base + take) as u16;
        return Ok(take);
    }
    Err(ERR_WOULD_BLOCK)
}

/// Blocking pipe write (drops the FS lock across sleeps).
pub fn write(id: u16, data: &[u8]) -> Result<usize, u64> {
    loop {
        let r = {
            let _g = crate::arch::IrqLock::acquire(&FS_LOCK);
            write_locked(id as usize, data)
        };
        match r {
            Err(e) if e == ERR_WOULD_BLOCK => {
                if crate::process::pending_handled(task::current_slot()) {
                    return Err(SYS_ERR_INTR);
                }
                task::sleep_ms(1);
            }
            other => return other,
        }
    }
}
