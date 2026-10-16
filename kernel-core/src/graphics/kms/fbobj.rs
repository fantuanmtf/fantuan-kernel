//! The dumb-buffer pool and the framebuffer registry (the ADDFB/RMFB half of
//! the KMS-like contract). A child of `graphics::kms`; see `super` for the
//! memory-source and single-writer arguments.
//!
//! Both tables take `super::LOCK` rather than a lock each: `dumb_destroy` walks
//! the FB table to invalidate anything pointing at the buffer it is freeing, so
//! the two are always read together.

use super::{LOCK, MAX_DUMB, MAX_FB};
use crate::arch::IrqLock;
use crate::frame;
use crate::graphics::{FbInfo, Format};
use crate::mem;

#[derive(Clone, Copy)]
struct DumbSlot {
    phys: u64,
    frames: usize,
    fb: FbInfo,
    used: bool,
}

#[derive(Clone, Copy)]
struct FbSlot {
    handle: u32,
    fb: FbInfo,
    used: bool,
}

const NULL_FB: FbInfo = FbInfo {
    base: core::ptr::null_mut(),
    width: 0,
    height: 0,
    pitch: 0,
    format: Format::Bpp32,
};
const DUMB_EMPTY: DumbSlot = DumbSlot { phys: 0, frames: 0, fb: NULL_FB, used: false };
const FB_EMPTY: FbSlot = FbSlot { handle: 0, fb: NULL_FB, used: false };

static mut DUMB: [DumbSlot; MAX_DUMB] = [DUMB_EMPTY; MAX_DUMB];
static mut FBS: [FbSlot; MAX_FB] = [FB_EMPTY; MAX_FB];

fn dumb() -> &'static mut [DumbSlot; MAX_DUMB] {
    unsafe { &mut *core::ptr::addr_of_mut!(DUMB) }
}

fn fbs() -> &'static mut [FbSlot; MAX_FB] {
    unsafe { &mut *core::ptr::addr_of_mut!(FBS) }
}

fn bytes_per_pixel(format: Format) -> u32 {
    format.bpp() / 8
}

fn dumb_slot(handle: u32) -> Option<usize> {
    if handle == 0 {
        return None;
    }
    let i = (handle - 1) as usize;
    (i < MAX_DUMB).then_some(i)
}

fn fb_slot(fb_id: u32) -> Option<usize> {
    if fb_id == 0 {
        return None;
    }
    let i = (fb_id - 1) as usize;
    (i < MAX_FB).then_some(i)
}

/// Allocate a dumb buffer (w x h at `format` bpp); returns a 1-based handle,
/// or None when the geometry is invalid, the table is full or the frame
/// allocator cannot satisfy the contiguous run.
pub fn dumb_create(width: u32, height: u32, format: Format) -> Option<u32> {
    if width == 0 || height == 0 {
        return None;
    }
    let bpp = bytes_per_pixel(format);
    let bytes = (width as u64).checked_mul(height as u64)?.checked_mul(bpp as u64)?;
    let frames = ((bytes + frame::FRAME_SIZE - 1) / frame::FRAME_SIZE) as usize;
    // Reserve a slot first so a pool-full failure cannot leak frames.
    let slot = {
        let _g = IrqLock::acquire(&LOCK);
        (0..MAX_DUMB).find(|&i| !dumb()[i].used)?
    };
    let phys = frame::get().alloc_contiguous(frames)?;
    let base = mem::phys_to_virt(phys) as *mut u8;
    let fb = FbInfo { base, width, height, pitch: width * bpp, format };
    let _g = IrqLock::acquire(&LOCK);
    dumb()[slot] = DumbSlot { phys, frames, fb, used: true };
    Some((slot + 1) as u32)
}

/// Destroy a dumb buffer, freeing its frames and invalidating any FB that
/// still references it.
pub fn dumb_destroy(handle: u32) -> bool {
    let Some(slot) = dumb_slot(handle) else {
        return false;
    };
    let (phys, frames, used) = {
        let _g = IrqLock::acquire(&LOCK);
        let s = &dumb()[slot];
        (s.phys, s.frames, s.used)
    };
    if !used {
        return false;
    }
    {
        let _g = IrqLock::acquire(&LOCK);
        for i in 0..MAX_FB {
            if fbs()[i].used && fbs()[i].handle == handle {
                fbs()[i] = FB_EMPTY;
            }
        }
    }
    for i in 0..frames {
        frame::get().free(phys + i as u64 * frame::FRAME_SIZE);
    }
    let _g = IrqLock::acquire(&LOCK);
    dumb()[slot] = DUMB_EMPTY;
    true
}

/// The dumb buffer's render surface, or None when the handle is invalid.
pub fn dumb_info(handle: u32) -> Option<FbInfo> {
    let slot = dumb_slot(handle)?;
    let _g = IrqLock::acquire(&LOCK);
    let s = &dumb()[slot];
    s.used.then_some(s.fb)
}

/// Number of dumb-buffer slots currently in use.
pub fn dumb_used() -> usize {
    let _g = IrqLock::acquire(&LOCK);
    dumb().iter().filter(|s| s.used).count()
}

/// Register a dumb buffer as a framebuffer object; returns a 1-based fb id.
/// Geometry is validated against the buffer: the pitch must cover the width,
/// the described region must fit in the allocated frames, and the format must
/// match the buffer's (the blit path does no conversion).
pub fn addfb(handle: u32, width: u32, height: u32, pitch: u32, format: Format) -> Option<u32> {
    let Some(slot) = dumb_slot(handle) else {
        return None;
    };
    let buf = {
        let _g = IrqLock::acquire(&LOCK);
        let s = &dumb()[slot];
        if !s.used {
            return None;
        }
        *s
    };
    if width == 0 || height == 0 || format != buf.fb.format {
        return None;
    }
    let bpp = bytes_per_pixel(format);
    if pitch < width * bpp {
        return None;
    }
    let needed = (pitch as u64) * (height as u64);
    let cap = (buf.frames as u64) * frame::FRAME_SIZE;
    if needed > cap {
        return None;
    }
    let fslot = {
        let _g = IrqLock::acquire(&LOCK);
        (0..MAX_FB).find(|&i| !fbs()[i].used)?
    };
    let fb = FbInfo { base: buf.fb.base, width, height, pitch, format };
    let _g = IrqLock::acquire(&LOCK);
    fbs()[fslot] = FbSlot { handle, fb, used: true };
    Some((fslot + 1) as u32)
}

/// Remove a framebuffer object.
pub fn rmfb(fb_id: u32) -> bool {
    let Some(slot) = fb_slot(fb_id) else {
        return false;
    };
    let _g = IrqLock::acquire(&LOCK);
    if !fbs()[slot].used {
        return false;
    }
    fbs()[slot] = FB_EMPTY;
    true
}

/// The framebuffer object's validated surface, or None when the id is invalid.
pub fn fb_info(fb_id: u32) -> Option<FbInfo> {
    let slot = fb_slot(fb_id)?;
    let _g = IrqLock::acquire(&LOCK);
    let s = &fbs()[slot];
    s.used.then_some(s.fb)
}
