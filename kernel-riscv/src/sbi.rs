//! SBI calls (M9.2b): the TIME extension via `ecall` from S-mode.
//! OpenSBI returns (error, value) in a0/a1; every call is checked.

use core::arch::asm;

pub const SBI_EXT_TIME: usize = 0x5449_4D45; // "TIME"

/// Set the next S-mode timer interrupt (absolute `time` CSR value).
pub fn set_timer(stime: u64) -> Result<(), usize> {
    let (err, _) = call(SBI_EXT_TIME, 0, stime as usize, 0, 0);
    if err == 0 {
        Ok(())
    } else {
        Err(err)
    }
}

/// Raw ecall with one argument; returns (error, value).
pub fn call(eid: usize, fid: usize, a0: usize, a1: usize, a2: usize) -> (usize, usize) {
    let mut ra0 = a0;
    let mut ra1 = a1;
    unsafe {
        asm!(
            "ecall",
            in("a7") eid,
            in("a6") fid,
            inout("a0") ra0,
            inout("a1") ra1,
            in("a2") a2,
            options(nostack),
        );
    }
    (ra0, ra1)
}
