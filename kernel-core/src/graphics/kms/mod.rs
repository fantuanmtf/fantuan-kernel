//! M13-4 dumb-buffer objects and the KMS-like contract (ADDFB/SETCRTC/PAGE_FLIP)
//! plus the single-CRTC scanout state (docs/M13_GRAPHICS.md §4). Internal API;
//! the userland boundary arrives with the POSIX graph layer (M14).
//!
//! Memory source and bounds: the dumb-buffer pool is a fixed table of MAX_DUMB
//! slots whose memory comes from the shared frame allocator
//! (`kernel-core::frame`, `alloc_contiguous`, freed frame-by-frame on destroy)
//! and is reached through the kernel's phys->virt alias (`kernel-core::mem`).
//! A buffer is exactly ceil(w*h*bpp/4096) contiguous frames; `dumb_create`
//! fails when the run cannot be satisfied or the table is full. The pool, the
//! FB table and the CRTC are single-writer (the graphics demo task) but hold
//! an IRQ-safe lock so the layout stays safe when the DRM fd path (M14) adds
//! a second consumer.
//!
//! Split by responsibility in 2026-10, ahead of M13-5: the ioctl surface that
//! step adds would otherwise have pushed this file past the 300-line rule at
//! 298. `fbobj` owns the dumb-buffer pool and the framebuffer registry, `crtc`
//! owns the scanout and the modeset path. The lock below is shared by both —
//! one lock, not two, because the tables are read together (`dumb_destroy`
//! invalidates FBs pointing at the buffer) and splitting it would make the
//! single-writer argument above untrue. M13-5 adds `ioctl.rs` (DRM_IOCTL_*
//! numbers and request structs) and `connector.rs` (EDID, modes, the synthetic
//! fallback) as further children here.

use core::sync::atomic::AtomicBool;

use super::event;

pub mod crtc;
pub mod fbobj;

pub use crtc::{crtc_fb, page_flip, scanout, set_scanout, setcrtc};
pub use fbobj::{addfb, dumb_create, dumb_destroy, dumb_info, dumb_used, fb_info, rmfb};

pub const MAX_DUMB: usize = 8;
pub const MAX_FB: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KmsError {
    NoScanout,
    BadFb,
    GeometryMismatch,
}

impl KmsError {
    pub const fn code(self) -> u16 {
        match self {
            KmsError::NoScanout => event::code::NO_SCANOUT,
            KmsError::BadFb => event::code::BAD_FB,
            KmsError::GeometryMismatch => event::code::GEOMETRY_MISMATCH,
        }
    }
}

/// The single lock over the dumb pool, the FB registry and the CRTC.
/// Private to `kms`; the child modules reach it as `super::LOCK`, which is how
/// one lock stays one lock.
static LOCK: AtomicBool = AtomicBool::new(false);
