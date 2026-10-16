//! The single CRTC: the scanout surface, the modeset path (SETCRTC) and page
//! flips, with the gfx events they emit. A child of `graphics::kms`; see
//! `super` for the memory-source and single-writer arguments.
//!
//! `present` is where a validated FB meets the CRTC mode, so both `setcrtc`
//! and `page_flip` share the geometry check and the ERROR event it emits.

use super::fbobj::fb_info;
use super::{KmsError, LOCK};
use crate::arch::IrqLock;
use crate::graphics::event::{self, GfxEvent};
use crate::graphics::{FbInfo, Rect};

#[derive(Clone, Copy)]
struct Crtc {
    device: Option<FbInfo>,
    fb_id: u32,
    seq: u32,
}

const CRTC_EMPTY: Crtc = Crtc { device: None, fb_id: 0, seq: 0 };

static mut CRTC: Crtc = CRTC_EMPTY;

fn crtc() -> &'static mut Crtc {
    unsafe { &mut *core::ptr::addr_of_mut!(CRTC) }
}

/// Install the single CRTC's scanout surface (called once by the console).
pub fn set_scanout(device: FbInfo) {
    let _g = IrqLock::acquire(&LOCK);
    crtc().device = Some(device);
}

/// The installed scanout surface, or None before the console brings one up.
pub fn scanout() -> Option<FbInfo> {
    let _g = IrqLock::acquire(&LOCK);
    crtc().device
}

/// The fb id currently bound to the CRTC (0 = none).
pub fn crtc_fb() -> u32 {
    let _g = IrqLock::acquire(&LOCK);
    crtc().fb_id
}

/// Copy a validated fb onto the scanout; the FB must match the CRTC mode.
fn present(fb: FbInfo, dev: FbInfo) -> Result<(), KmsError> {
    if fb.width != dev.width || fb.height != dev.height || fb.format != dev.format {
        return Err(KmsError::GeometryMismatch);
    }
    let _ = dev.blit(fb.base, fb.pitch, Rect::new(0, 0, dev.width, dev.height));
    Ok(())
}

/// Bind an fb id to the CRTC and present it (a modeset). Emits an ERROR event
/// and returns Err on a bad id or a geometry/mode mismatch.
pub fn setcrtc(fb_id: u32) -> Result<(), KmsError> {
    let fb = fb_info(fb_id).ok_or(KmsError::BadFb)?;
    let dev = scanout().ok_or(KmsError::NoScanout)?;
    if let Err(e) = present(fb, dev) {
        event::push(GfxEvent::error(fb_id, e.code()));
        return Err(e);
    }
    let _g = IrqLock::acquire(&LOCK);
    crtc().fb_id = fb_id;
    Ok(())
}

/// Swap the CRTC to another fb id, present it, and emit a flip-complete event
/// carrying the monotonic flip sequence number. Emits an ERROR event and
/// returns Err on a bad id or a geometry/mode mismatch.
pub fn page_flip(fb_id: u32) -> Result<(), KmsError> {
    let fb = fb_info(fb_id).ok_or(KmsError::BadFb)?;
    let dev = scanout().ok_or(KmsError::NoScanout)?;
    if let Err(e) = present(fb, dev) {
        event::push(GfxEvent::error(fb_id, e.code()));
        return Err(e);
    }
    let seq = {
        let _g = IrqLock::acquire(&LOCK);
        crtc().fb_id = fb_id;
        crtc().seq += 1;
        crtc().seq
    };
    event::push(GfxEvent::flip_complete(fb_id, seq));
    Ok(())
}
