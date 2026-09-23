//! Kernel framebuffer demo (M13-2): one bounded frame of animation — a moving
//! box plus an 8-bit counter drawn into a fixed bottom band. Pure software
//! drawing through `FbInfo`/`Damage`; the caller owns the present and the
//! frame loop. This is the shared proof-of-contract drawing both the x86_64
//! GOP and i686 VBE demo tasks reuse (docs/M13_GRAPHICS.md §6).

use super::{Damage, FbInfo, Rect};

const BAND_H: u32 = 96;
const BOX_SIZE: u32 = 32;
const STEP: u64 = 24;
const CELL: u32 = 8;
const BG: u32 = 0x0000_0000;
const BOX_FG: u32 = 0x00FF_FFFF;
const BIT_FG: u32 = 0x0000_FF00;

/// Cursor sprite edge (a solid square), drawn in magenta (R == B, symmetric in
/// RGB/BGR byte order like the other demo colors).
pub const CURSOR_SIZE: u32 = 8;
pub const CURSOR_FG: u32 = 0x00FF_00FF;

pub struct DemoStats {
    pub bbox: Rect,
    pub rects: usize,
    pub area: u64,
}

/// Draw frame `frame` into `fb` (a persistent off-screen surface with double
/// buffering, so the previous box position is erased first) and mark every
/// touched rect in `d`. Returns the frame's damage summary.
pub fn demo_frame(fb: FbInfo, d: &mut Damage, frame: u64) -> DemoStats {
    let band_h = BAND_H.min(fb.height);
    let y0 = fb.height - band_h;
    let span = (fb.width.saturating_sub(BOX_SIZE).saturating_add(1)).max(1) as u64;
    let x = ((frame.wrapping_mul(STEP)) % span) as u32;
    let px = ((frame.saturating_sub(1).wrapping_mul(STEP)) % span) as u32;

    let mut bbox = Rect::new(0, 0, 0, 0);
    let mut touch = |r: Option<Rect>| {
        if let Some(c) = r {
            d.add(c);
            bbox = bbox.union(c);
        }
    };

    touch(fb.fill(Rect::new(px, y0, BOX_SIZE, BOX_SIZE), BG));
    touch(fb.fill(Rect::new(x, y0, BOX_SIZE, BOX_SIZE), BOX_FG));

    let cy = y0 + BOX_SIZE + 8;
    let cw = CELL * 8 + 7;
    touch(fb.fill(Rect::new(0, cy, cw, CELL), BG));
    for bit in 0..8u32 {
        if (frame >> bit) & 1 == 1 {
            touch(fb.fill(Rect::new(bit * (CELL + 1), cy, CELL, CELL), BIT_FG));
        }
    }

    DemoStats {
        bbox,
        rects: d.len(),
        area: bbox.area(),
    }
}

/// Draw the cursor sprite (a solid CURSOR_SIZE square) at (x, y), clamped to
/// the surface, marking damage. Returns the drawn rect. The caller erases the
/// previous position (the shared surface is double-buffered and persistent).
pub fn demo_cursor(fb: FbInfo, d: &mut Damage, x: u32, y: u32) -> Option<Rect> {
    let cx = x.min(fb.width.saturating_sub(CURSOR_SIZE));
    let cy = y.min(fb.height.saturating_sub(CURSOR_SIZE));
    let r = fb.fill(Rect::new(cx, cy, CURSOR_SIZE, CURSOR_SIZE), CURSOR_FG)?;
    d.add(r);
    Some(r)
}
