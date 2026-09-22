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
}
