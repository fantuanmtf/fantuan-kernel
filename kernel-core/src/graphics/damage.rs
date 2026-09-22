//! Bounded dirty-rect tracker, the present/scanout seam and the pure-logic
//! self-test (M13-1).

use super::{FbInfo, Format, Rect};

pub const MAX_DAMAGE: usize = 16;

pub struct Damage {
    rects: [Rect; MAX_DAMAGE],
    len: usize,
}

impl Damage {
    pub const fn new() -> Damage {
        Damage {
            rects: [Rect::new(0, 0, 0, 0); MAX_DAMAGE],
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn rects(&self) -> &[Rect] {
        &self.rects[..self.len]
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Union of every dirty rect (the flush region); `None` when empty.
    pub fn bbox(&self) -> Option<Rect> {
        let mut it = self.rects[..self.len].iter().copied();
        let first = it.next()?;
        Some(it.fold(first, |a, b| a.union(b)))
    }

    /// Sum of dirty-rect areas (entries are non-overlapping until the list
    /// fills, so this is the true touched area for the common case).
    pub fn area(&self) -> u64 {
        self.rects[..self.len].iter().map(|r| r.area()).sum()
    }

    /// Add a dirty rect, coalescing with any overlapping entry. The list is
    /// bounded: when full, a non-overlapping rect merges into the entry whose
    /// union grows least, so the dirty region stays fully covered.
    pub fn add(&mut self, r: Rect) {
        if r.is_empty() {
            return;
        }
        let mut best = 0usize;
        let mut best_growth = u64::MAX;
        for i in 0..self.len {
            let cur = self.rects[i];
            if cur.overlaps(r) {
                self.rects[i] = cur.union(r);
                return;
            }
            let growth = cur.union(r).area().saturating_sub(cur.area());
            if growth < best_growth {
                best_growth = growth;
                best = i;
            }
        }
        if self.len < MAX_DAMAGE {
            self.rects[self.len] = r;
            self.len += 1;
        } else if self.len > 0 {
            self.rects[best] = self.rects[best].union(r);
        }
    }
}

pub trait Present {
    /// Flush `fb`'s damaged rects to the scanout. Single-buffered (M13-1) is
    /// the identity; a double-buffered present (M13-2) copies the rects.
    fn present(&self, fb: &FbInfo, damage: &mut Damage);
}

pub struct DirectPresent;

impl Present for DirectPresent {
    fn present(&self, _fb: &FbInfo, damage: &mut Damage) {
        damage.clear();
    }
}

/// Double-buffered present (M13-2): the render surface `fb` is an off-screen
/// buffer, and each damaged rect is copied onto the scanout `device` surface
/// before the list is cleared. Only the dirty rects are touched; the untouched
/// device pixels keep their previous content.
pub struct BufferedPresent {
    device: FbInfo,
}

impl BufferedPresent {
    pub const fn new(device: FbInfo) -> BufferedPresent {
        BufferedPresent { device }
    }
}

impl Present for BufferedPresent {
    fn present(&self, fb: &FbInfo, damage: &mut Damage) {
        // blit takes the source rect's top-left (not the surface base), so
        // point each damaged rect at its own off-screen origin.
        let bytes = (fb.bpp() / 8) as usize;
        for r in damage.rects() {
            let src = unsafe {
                fb.base
                    .add(r.y as usize * fb.pitch as usize + r.x as usize * bytes)
            };
            let _ = self.device.blit(src, fb.pitch, *r);
        }
        damage.clear();
    }
}

/// Pure-logic self-test for clipping and damage merging (no drawing).
pub fn selftest() -> bool {
    let whole = Rect::new(0, 0, 8, 4);
    if whole.intersect(Rect::new(6, 3, 4, 4)) != Rect::new(6, 3, 2, 1) {
        return false;
    }
    if !whole.intersect(Rect::new(8, 0, 4, 4)).is_empty() {
        return false;
    }

    let mut d = Damage::new();
    d.add(Rect::new(0, 0, 10, 10));
    d.add(Rect::new(5, 5, 10, 10));
    if d.len() != 1 || d.rects()[0] != Rect::new(0, 0, 15, 15) {
        return false;
    }

    let mut d = Damage::new();
    for k in 0..MAX_DAMAGE {
        d.add(Rect::new(k as u32 * 10, 0, 4, 4));
    }
    if d.len() != MAX_DAMAGE {
        return false;
    }
    d.add(Rect::new(1000, 1000, 4, 4));
    if d.len() != MAX_DAMAGE {
        return false;
    }
    let covered = d
        .rects()
        .iter()
        .any(|r| r.x <= 1000 && r.right() >= 1004 && r.y <= 1000 && r.bottom() >= 1004);
    if !covered {
        return false;
    }

    let mut buf = [0u8; 8 * 4 * 4];
    let fb = FbInfo {
        base: buf.as_mut_ptr(),
        width: 8,
        height: 4,
        pitch: 32,
        format: Format::Bpp32,
    };
    let c = match fb.fill(Rect::new(4, 2, 100, 100), 0x00FF_FFFF) {
        Some(c) => c,
        None => return false,
    };
    if c != Rect::new(4, 2, 4, 2) {
        return false;
    }
    buf[0] == 0 && buf[(3 * 32 + 7 * 4) as usize] == 0xFF
        && buffered_present_test()
}

/// BufferedPresent round trip: a rect drawn into the off-screen surface is
/// copied to the device surface on present, the list is cleared, and pixels
/// outside the damaged rect stay untouched.
fn buffered_present_test() -> bool {
    let mut back = [0u8; 8 * 4 * 4];
    let mut dev = [0u8; 8 * 4 * 4];
    let off = FbInfo {
        base: back.as_mut_ptr(),
        width: 8,
        height: 4,
        pitch: 32,
        format: Format::Bpp32,
    };
    let devfb = FbInfo {
        base: dev.as_mut_ptr(),
        width: 8,
        height: 4,
        pitch: 32,
        format: Format::Bpp32,
    };
    if off.fill(Rect::new(1, 1, 2, 2), 0x00FF_0000).is_none() {
        return false;
    }
    let mut d = Damage::new();
    d.add(Rect::new(1, 1, 2, 2));
    let bp = BufferedPresent::new(devfb);
    bp.present(&off, &mut d);
    if !d.is_empty() {
        return false;
    }
    unsafe {
        let hit = dev.as_ptr().add(1 * 32 + 1 * 4) as *const u32;
        let miss = dev.as_ptr() as *const u32;
        hit.read_volatile() == 0x00FF_0000 && miss.read_volatile() == 0
    }
}
