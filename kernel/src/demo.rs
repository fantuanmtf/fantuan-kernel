//! M3 demo tasks (kernel-mode): each prints through the syscall layer and
//! sleeps on its own period. Split out of main.rs per DESIGN.md §13.4a —
//! main.rs owns boot orchestration only.

use crate::syscall;

fn push_str(buf: &mut [u8], mut off: usize, s: &str) -> usize {
    for &b in s.as_bytes() {
        if off < buf.len() {
            buf[off] = b;
            off += 1;
        }
    }
    off
}

fn push_u64(buf: &mut [u8], mut off: usize, mut v: u64) -> usize {
    if v == 0 {
        buf[off] = b'0';
        return off + 1;
    }
    let mut tmp = [0u8; 20];
    let mut n = 0;
    while v > 0 {
        tmp[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
    }
    while n > 0 {
        n -= 1;
        buf[off] = tmp[n];
        off += 1;
    }
    off
}

/// Every task: report its identity through the syscall layer, then sleep
/// (woken by the scheduler's deadline check). Printing stops after a few
/// rounds so an interactive shell on the same console is not flooded; the
/// tasks keep sleeping so the scheduler still has work to rotate.
const DEMO_PRINTS: u64 = 3;

fn demo_task(tag: u64) -> ! {
    let mut n = 0u64;
    loop {
        if n < DEMO_PRINTS {
            let tid = syscall::syscall(syscall::SYS_GET_TID, 0, 0, 0, 0, 0);
            let mut buf = [0u8; 96];
            let mut off = 0;
            off = push_str(&mut buf, off, "task ");
            off = push_u64(&mut buf, off, tag);
            off = push_str(&mut buf, off, " (tid ");
            off = push_u64(&mut buf, off, tid);
            off = push_str(&mut buf, off, "): hello ");
            off = push_u64(&mut buf, off, n);
            buf[off] = b'\n';
            off += 1;
            syscall::syscall(syscall::SYS_WRITE, buf.as_ptr() as u64, off as u64, 0, 0, 0);
        } else if n == DEMO_PRINTS {
            let mut buf = [0u8; 64];
            let mut off = 0;
            off = push_str(&mut buf, off, "task ");
            off = push_u64(&mut buf, off, tag);
            off = push_str(&mut buf, off, ": quiet (scheduler keeps rotating)\n");
            syscall::syscall(syscall::SYS_WRITE, buf.as_ptr() as u64, off as u64, 0, 0, 0);
        }
        n += 1;
        // Different periods per task; the scheduler's deadline check wakes us.
        syscall::syscall(syscall::SYS_SLEEP_MS, 250 + tag * 150, 0, 0, 0, 0);
    }
}

pub fn demo_1() -> ! {
    demo_task(1)
}
pub fn demo_2() -> ! {
    demo_task(2)
}
pub fn demo_3() -> ! {
    demo_task(3)
}
