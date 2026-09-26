//! M13-4 graphics event ring: a bounded, lock-protected ring of KMS events
//! (flip-complete / error) with a stable `GfxEvent` layout guarded by a
//! self-test, mirroring `input_ring`. One producer (the CRTC flip path) and
//! one consumer (the graphics demo; the DRM fd read later). No allocation;
//! drop-oldest under pressure with a monotonic drop counter. Gated by
//! CONFIG_GRAPHICS (docs/M13_GRAPHICS.md §4).

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::arch;
use crate::time;

/// Byte size of a `GfxEvent` (the ABI self-test guards this).
pub const GFX_EVENT_SIZE: usize = 32;
/// Number of events the ring holds before it starts dropping the oldest.
pub const GFX_RING_CAPACITY: usize = 64;

/// Event kind tags (append-only; never renumber).
pub mod kind {
    pub const FLIP_COMPLETE: u16 = 1;
    pub const ERROR: u16 = 2;
}

/// Error codes carried by `kind::ERROR` events (append-only).
pub mod code {
    pub const NONE: u16 = 0;
    pub const NO_SCANOUT: u16 = 1;
    pub const BAD_FB: u16 = 2;
    pub const GEOMETRY_MISMATCH: u16 = 3;
}

/// Frozen 32-byte graphics event. Field order is part of the contract:
///
/// | offset | size | field      | meaning                              |
/// |--------|------|------------|--------------------------------------|
/// | 0      | 8    | ts         | monotonic nanoseconds (0 = no clock) |
/// | 8      | 2    | kind       | kind::FLIP_COMPLETE or kind::ERROR   |
/// | 10     | 2    | code       | error code (code::NONE for flips)    |
/// | 12     | 4    | fb_id      | framebuffer id the event refers to   |
/// | 16     | 4    | seq        | flip sequence number (monotonic)     |
/// | 20     | 12   | _reserved  | zero; reserved for extension         |
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct GfxEvent {
    pub ts: u64,
    pub kind: u16,
    pub code: u16,
    pub fb_id: u32,
    pub seq: u32,
    pub _reserved: [u8; 12],
}

impl GfxEvent {
    pub const EMPTY: GfxEvent = GfxEvent {
        ts: 0,
        kind: 0,
        code: 0,
        fb_id: 0,
        seq: 0,
        _reserved: [0; 12],
    };

    pub const fn flip_complete(fb_id: u32, seq: u32) -> GfxEvent {
        GfxEvent {
            ts: 0,
            kind: kind::FLIP_COMPLETE,
            code: code::NONE,
            fb_id,
            seq,
            _reserved: [0; 12],
        }
    }

    pub const fn error(fb_id: u32, code: u16) -> GfxEvent {
        GfxEvent {
            ts: 0,
            kind: kind::ERROR,
            code,
            fb_id,
            seq: 0,
            _reserved: [0; 12],
        }
    }
}

struct Ring {
    events: [GfxEvent; GFX_RING_CAPACITY],
    head: usize,
    tail: usize,
    len: usize,
}

const RING_INIT: Ring = Ring {
    events: [GfxEvent::EMPTY; GFX_RING_CAPACITY],
    head: 0,
    tail: 0,
    len: 0,
};

static LOCK: AtomicBool = AtomicBool::new(false);
static DROPPED: AtomicU64 = AtomicU64::new(0);
static mut RING: Ring = RING_INIT;

/// Push one event. The ring stamps the monotonic timestamp. When full, the
/// oldest entry is dropped.
pub fn push(mut ev: GfxEvent) {
    let _g = arch::IrqLock::acquire(&LOCK);
    ev.ts = time::now_ns();
    let r = unsafe { &mut *core::ptr::addr_of_mut!(RING) };
    if r.len == GFX_RING_CAPACITY {
        r.tail = (r.tail + 1) % GFX_RING_CAPACITY;
        DROPPED.fetch_add(1, Ordering::Relaxed);
    } else {
        r.len += 1;
    }
    r.events[r.head] = ev;
    r.head = (r.head + 1) % GFX_RING_CAPACITY;
}

/// Pop the oldest event for the single consumer; None when empty.
pub fn pop() -> Option<GfxEvent> {
    let _g = arch::IrqLock::acquire(&LOCK);
    let r = unsafe { &mut *core::ptr::addr_of_mut!(RING) };
    if r.len == 0 {
        return None;
    }
    let ev = r.events[r.tail];
    r.tail = (r.tail + 1) % GFX_RING_CAPACITY;
    r.len -= 1;
    Some(ev)
}

/// Number of events dropped because the ring was full (monotonic).
pub fn dropped() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

/// Boot-time self-test: ABI size guard, FIFO order, an error round trip and
/// the drop-oldest policy. Leaves the ring empty.
pub fn selftest() -> bool {
    if core::mem::size_of::<GfxEvent>() != GFX_EVENT_SIZE {
        return false;
    }
    while pop().is_some() {}

    for i in 0..8u32 {
        push(GfxEvent::flip_complete(i + 1, i));
    }
    for i in 0..8u32 {
        let Some(ev) = pop() else {
            return false;
        };
        if ev.kind != kind::FLIP_COMPLETE || ev.fb_id != i + 1 || ev.seq != i {
            return false;
        }
    }

    push(GfxEvent::error(7, code::GEOMETRY_MISMATCH));
    let Some(ev) = pop() else {
        return false;
    };
    if ev.kind != kind::ERROR || ev.fb_id != 7 || ev.code != code::GEOMETRY_MISMATCH {
        return false;
    }

    for _ in 0..GFX_RING_CAPACITY {
        push(GfxEvent::flip_complete(1, 1));
    }
    let before = dropped();
    push(GfxEvent::flip_complete(2, 2));
    if dropped() != before + 1 {
        return false;
    }
    for _ in 0..GFX_RING_CAPACITY {
        if pop().is_none() {
            return false;
        }
    }
    pop().is_none()
}
