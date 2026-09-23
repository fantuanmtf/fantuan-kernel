//! M13-3 input event ring: one bounded, lock-protected ring of input events
//! shared by the PS/2 keyboard (IRQ1) and mouse (IRQ12) producers and a single
//! consumer (the graphics demo; later /dev/input/event0). No allocation; the
//! ring is a static table. Drop-oldest under pressure, with a monotonic drop
//! counter. Gated by CONFIG_GRAPHICS so the minimal/net profiles link none of
//! it. The `InputEvent` layout is the frozen M13-3 contract (docs/M13_GRAPHICS.md
//! §3); consumers in M13-4/V-f compile against these offsets.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::arch;
use crate::time;

/// Byte size of an `InputEvent` (the ABI self-test guards this).
pub const INPUT_EVENT_SIZE: usize = 32;
/// Number of events the ring holds before it starts dropping the oldest.
pub const RING_CAPACITY: usize = 64;

/// Event kind tags (append-only; never renumber).
pub mod kind {
    pub const KEY: u16 = 1;
    pub const POINTER: u16 = 2;
}

/// Key `flags` values.
pub const KEY_RELEASED: u8 = 0;
pub const KEY_PRESSED: u8 = 1;
/// Scancode set the keyboard driver reports (set 1 after 8042 translation).
pub const SCANCODE_SET: u8 = 1;

/// Frozen 32-byte input event. Field order is part of the contract:
///
/// | offset | size | field      | meaning                               |
/// |--------|------|------------|---------------------------------------|
/// | 0      | 8    | ts         | monotonic nanoseconds (0 = no clock)  |
/// | 8      | 2    | kind       | kind::KEY or kind::POINTER            |
/// | 10     | 1    | flags      | key: KEY_PRESSED/KEY_RELEASED         |
/// | 11     | 1    | set        | key: scancode set (SCANCODE_SET)      |
/// | 12     | 1    | scancode   | key: make code                        |
/// | 13     | 1    | ascii      | key: translated ASCII (0 = none)      |
/// | 14     | 1    | buttons    | pointer: bit 0 left, 1 right, 2 middle|
/// | 16     | 2    | dx         | pointer: relative X (signed)          |
/// | 18     | 2    | dy         | pointer: relative Y (signed)          |
/// | 20     | 12   | _reserved  | zero; reserved for extension          |
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    pub ts: u64,
    pub kind: u16,
    pub flags: u8,
    pub set: u8,
    pub scancode: u8,
    pub ascii: u8,
    pub buttons: u8,
    pub dx: i16,
    pub dy: i16,
    pub _reserved: [u8; 12],
}

impl InputEvent {
    pub const EMPTY: InputEvent = InputEvent {
        ts: 0,
        kind: 0,
        flags: 0,
        set: 0,
        scancode: 0,
        ascii: 0,
        buttons: 0,
        dx: 0,
        dy: 0,
        _reserved: [0; 12],
    };

    pub const fn key(set: u8, scancode: u8, pressed: bool, ascii: u8) -> InputEvent {
        InputEvent {
            ts: 0,
            kind: kind::KEY,
            flags: if pressed { KEY_PRESSED } else { KEY_RELEASED },
            set,
            scancode,
            ascii,
            buttons: 0,
            dx: 0,
            dy: 0,
            _reserved: [0; 12],
        }
    }

    pub const fn pointer(dx: i16, dy: i16, buttons: u8) -> InputEvent {
        InputEvent {
            ts: 0,
            kind: kind::POINTER,
            flags: 0,
            set: 0,
            scancode: 0,
            ascii: 0,
            buttons,
            dx,
            dy,
            _reserved: [0; 12],
        }
    }
}

struct Ring {
    events: [InputEvent; RING_CAPACITY],
    head: usize,
    tail: usize,
    len: usize,
}

const RING_INIT: Ring = Ring {
    events: [InputEvent::EMPTY; RING_CAPACITY],
    head: 0,
    tail: 0,
    len: 0,
};

static LOCK: AtomicBool = AtomicBool::new(false);
static DROPPED: AtomicU64 = AtomicU64::new(0);
static mut RING: Ring = RING_INIT;

/// Push one event. The ring stamps the monotonic timestamp. Producers are the
/// PS/2 IRQ handlers; the lock disables interrupts so an IRQ cannot preempt a
/// holder (single CPU). When full, the oldest entry is dropped.
pub fn push(mut ev: InputEvent) {
    let _g = arch::IrqLock::acquire(&LOCK);
    ev.ts = time::now_ns();
    let r = unsafe { &mut *core::ptr::addr_of_mut!(RING) };
    if r.len == RING_CAPACITY {
        // Full: drop the oldest entry.
        r.tail = (r.tail + 1) % RING_CAPACITY;
        DROPPED.fetch_add(1, Ordering::Relaxed);
    } else {
        r.len += 1;
    }
    r.events[r.head] = ev;
    r.head = (r.head + 1) % RING_CAPACITY;
}

/// Pop the oldest event for the single consumer; None when empty.
pub fn pop() -> Option<InputEvent> {
    let _g = arch::IrqLock::acquire(&LOCK);
    let r = unsafe { &mut *core::ptr::addr_of_mut!(RING) };
    if r.len == 0 {
        return None;
    }
    let ev = r.events[r.tail];
    r.tail = (r.tail + 1) % RING_CAPACITY;
    r.len -= 1;
    Some(ev)
}

/// Number of events dropped because the ring was full (monotonic).
pub fn dropped() -> u64 {
    DROPPED.load(Ordering::Relaxed)
}

/// Producer helpers used by the arch keyboard/mouse drivers.
pub fn push_key(scancode: u8, set: u8, pressed: bool, ascii: u8) {
    push(InputEvent::key(set, scancode, pressed, ascii));
}

pub fn push_pointer(dx: i16, dy: i16, buttons: u8) {
    push(InputEvent::pointer(dx, dy, buttons));
}

/// Boot-time self-test: ABI size guard, FIFO order, monotonic timestamps, a
/// pointer round trip and the drop-oldest policy. Leaves the ring empty.
pub fn selftest() -> bool {
    if core::mem::size_of::<InputEvent>() != INPUT_EVENT_SIZE {
        return false;
    }
    while pop().is_some() {}

    for i in 0..8u8 {
        push_key(i, SCANCODE_SET, true, i.wrapping_add(b'a'));
    }
    let mut last_ts = 0u64;
    for i in 0..8u8 {
        let Some(ev) = pop() else {
            return false;
        };
        if ev.kind != kind::KEY || ev.scancode != i || ev.flags != KEY_PRESSED {
            return false;
        }
        if ev.ts < last_ts {
            return false;
        }
        last_ts = ev.ts;
    }

    push_pointer(-5, 7, 0b101);
    let Some(ev) = pop() else {
        return false;
    };
    if ev.kind != kind::POINTER || ev.dx != -5 || ev.dy != 7 || ev.buttons != 0b101 {
        return false;
    }

    for i in 0..RING_CAPACITY {
        push_key(i as u8, SCANCODE_SET, true, 0);
    }
    let before = dropped();
    push_key(0x7F, SCANCODE_SET, true, 0);
    if dropped() != before + 1 {
        return false;
    }
    for i in 0..RING_CAPACITY {
        let Some(ev) = pop() else {
            return false;
        };
        let expected = if i < RING_CAPACITY - 1 { (i + 1) as u8 } else { 0x7F };
        if ev.scancode != expected {
            return false;
        }
    }
    pop().is_none()
}
