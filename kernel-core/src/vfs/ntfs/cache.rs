//! Bounded cluster cache for the NTFS read path (M12-4): two 4 KiB slots.
//!
//! The volume is mounted read-only, so cached bytes never go stale and a
//! racing fill is harmless. The lock is only held for the slot lookup and
//! the memcpy; the device read itself runs unlocked (the block drivers
//! poll, but holding IRQs off across a disk command is not this layer's
//! job). Single-core, so the IrqLock is what keeps the cache consistent
//! across a timer-preempted read.

use core::ffi::c_void;
use core::sync::atomic::AtomicBool;

use crate::arch::IrqLock;

extern "C" {
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
}

pub const MAX_CLUSTER: usize = 4096;
const SLOTS: usize = 2;

#[derive(Clone, Copy)]
struct Slot {
    valid: bool,
    lba: u64,
    len: usize,
    data: [u8; MAX_CLUSTER],
}

const EMPTY_SLOT: Slot = Slot { valid: false, lba: 0, len: 0, data: [0; MAX_CLUSTER] };

static LOCK: AtomicBool = AtomicBool::new(false);
static mut CACHE: [Slot; SLOTS] = [EMPTY_SLOT; SLOTS];
static mut NEXT: usize = 0;

/// Read one cluster at LBA, copying `out.len()` bytes from `skip` within it.
/// False when the device read fails; `skip`/`out` must stay inside the
/// cluster (the callers derive both from the cluster size).
pub fn read_cluster(lba: u64, clen: usize, skip: usize, out: &mut [u8]) -> bool {
    if clen == 0 || clen > MAX_CLUSTER || clen % 512 != 0 || skip + out.len() > clen {
        return false;
    }
    let cache = core::ptr::addr_of_mut!(CACHE);
    {
        let _g = IrqLock::acquire(&LOCK);
        for i in 0..SLOTS {
            let s = unsafe { &(*cache)[i] };
            if s.valid && s.lba == lba && s.len == clen {
                out.copy_from_slice(&s.data[skip..skip + out.len()]);
                return true;
            }
        }
    }
    let mut buf = [0u8; MAX_CLUSTER];
    if unsafe { blk_read(core::ptr::null_mut(), lba, buf.as_mut_ptr() as *mut c_void, clen / 512) } != 0 {
        return false;
    }
    out.copy_from_slice(&buf[skip..skip + out.len()]);
    let _g = IrqLock::acquire(&LOCK);
    let idx = unsafe { NEXT };
    unsafe {
        NEXT = (NEXT + 1) % SLOTS;
        (*cache)[idx] = Slot { valid: true, lba, len: clen, data: buf };
    }
    true
}
