//! Arch-neutral syscall semantics (M9.3c). Each kernel provides the entry
//! mechanism (x86 int 0x60, riscv ecall from U-mode) and the user-pointer
//! write bridge; the call numbers and behavior live here.

use fantuan_abi::{
    SYS_ERR_NOSYS, SYS_EXIT, SYS_GET_TID, SYS_OK, SYS_SLEEP_MS, SYS_VERSION, SYS_WRITE, SYS_YIELD,
};

use crate::task;

pub const ABI_VERSION: u64 = 1;

/// Debug-channel write bridge (bounded copy + output; arch-specific).
pub type WriteFn = fn(u64, u64) -> u64;

/// Dispatch one call. Returns the value to place in the result register.
pub fn dispatch(write: WriteFn, n: u64, a: &[u64; 5]) -> u64 {
    match n {
        SYS_VERSION => ABI_VERSION,
        SYS_GET_TID => task::current_id(),
        SYS_EXIT => task::exit(a[0]),
        SYS_SLEEP_MS => {
            task::sleep_ms(a[0]);
            SYS_OK
        }
        SYS_WRITE => write(a[0], a[1]),
        SYS_YIELD => {
            task::schedule();
            SYS_OK
        }
        _ => SYS_ERR_NOSYS,
    }
}
