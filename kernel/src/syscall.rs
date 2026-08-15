//! Own syscall ABI v1 (DESIGN.md §4.6).
//!
//! Mechanism: INT 0x60 (a plain IDT gate; ring-3 entry comes with M4).
//! Register ABI: rax = number, rdi..r8 = five arguments, rax = result.
//! Versioned dispatch: each call is registered with its own version so the
//! ABI can evolve per-call (capability negotiation). Unknown numbers return
//! ERR_NOSYS. v1 runs with kernel-mode callers only — arguments are trusted.

use crate::interrupts::InterruptFrame;
use crate::serial::{self, Serial};
use crate::task;

pub const SYSCALL_VECTOR: u64 = 0x60;
pub const ABI_VERSION: u64 = 1;

pub const SYS_VERSION: u64 = 0;
pub const SYS_EXIT: u64 = 1;
pub const SYS_SLEEP_MS: u64 = 2;
pub const SYS_WRITE: u64 = 3; // (buf, len): kernel debug channel (serial)
pub const SYS_GET_TID: u64 = 4;
pub const SYS_YIELD: u64 = 5;

pub const OK: u64 = 0;
pub const ERR_NOSYS: u64 = u64::MAX; // -1
pub const ERR_INVAL: u64 = u64::MAX - 1; // -2

extern "C" {
    fn syscall_trampoline(n: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64;
}

/// Call the kernel. The trampoline reserves scratch below rsp so the interrupt
/// frame cannot touch the caller's red zone.
#[inline]
pub fn syscall(n: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64 {
    unsafe { syscall_trampoline(n, a1, a2, a3, a4, a5) }
}

/// Called from isr_dispatch with the interrupt frame of INT 0x60.
pub fn dispatch(frame: &mut InterruptFrame) {
    let n = frame.rax;
    let result = match n {
        SYS_VERSION => ABI_VERSION,
        SYS_GET_TID => task::current_id(),
        SYS_EXIT => task::exit(frame.rdi),
        SYS_SLEEP_MS => {
            task::sleep_ms(frame.rdi);
            OK
        }
        SYS_WRITE => sys_write(frame.rdi, frame.rsi),
        SYS_YIELD => {
            task::schedule();
            OK
        }
        _ => ERR_NOSYS,
    };
    frame.rax = result;
}

fn sys_write(ptr: u64, len: u64) -> u64 {
    let len = len.min(512);
    if ptr == 0 {
        return ERR_INVAL;
    }
    // M3: kernel-mode callers are trusted; no address validation yet.
    let buf = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let s = Serial::new(serial::COM1);
    let _ = s.write(buf);
    OK
}
