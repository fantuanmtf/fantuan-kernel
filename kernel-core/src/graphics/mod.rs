//! Framebuffer core (M13-1): a platform-neutral software renderer over a
//! kernel-visible pixel surface, a bounded damage tracker and the
//! present/scanout seam. Pure software drawing with hard clipping to the
//! surface; no allocation; assert-free (invalid geometry clips to nothing).
//! Only the 32/24/16-bpp modes the consoles already render. Gated by
//! CONFIG_GRAPHICS; docs/M13_GRAPHICS.md §2.

use core::ptr;

pub mod damage;
pub mod demo;
pub use damage::{selftest, BufferedPresent, Damage, DirectPresent, Present, MAX_DAMAGE};
pub use demo::{demo_frame, DemoStats};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    Bpp32,
    Bpp24,
    Bpp16,
}

impl Format {
    pub const fn bpp(self) -> u32 {
        match self {
            Format::Bpp32 => 32,
            Format::Bpp24 => 24,
            Format::Bpp16 => 16,
        }
    }

    const fn bytes(self) -> usize {
        (self.bpp() / 8) as usize
    }

    /// Store a 0x00RRGGBB pixel at `p` (BGR byte order for Bpp24).
    pub unsafe fn store(self, p: *mut u8, pixel: u32) {
        match self {
            Format::Bpp32 => (p as *mut u32).write_volatile(pixel),
            Format::Bpp24 => {
                p.write((pixel & 0xFF) as u8);
                p.add(1).write(((pixel >> 8) & 0xFF) as u8);
                p.add(2).write(((pixel >> 16) & 0xFF) as u8);
            }
            Format::Bpp16 => {
                let v = (((pixel >> 19) & 0x1F) << 11)
                    | (((pixel >> 10) & 0x3F) << 5)
                    | ((pixel >> 3) & 0x1F);
                (p as *mut u16).write_volatile(v as u16);
            }
        }
    }

    /// Load a pixel as 0x00RRGGBB.
    pub unsafe fn load(self, p: *const u8) -> u32 {
        match self {
            Format::Bpp32 => (p as *const u32).read_volatile(),
            Format::Bpp24 => {
                p.read() as u32
                    | ((p.add(1).read() as u32) << 8)
                    | ((p.add(2).read() as u32) << 16)
            }
            Format::Bpp16 => {
                let v = (p as *const u16).read_volatile() as u32;
                (((v >> 11) & 0x1F) << 19) | (((v >> 5) & 0x3F) << 10) | ((v & 0x1F) << 3)
            }
        }
    }

    /// XOR a 0x00RRGGBB pixel into `p` (cursor overlay).
    pub unsafe fn xor(self, p: *mut u8, pixel: u32) {
        let cur = self.load(p);
        self.store(p, cur ^ pixel);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Rect {
        Rect { x, y, w, h }
    }

    pub const fn is_empty(self) -> bool {
        self.w == 0 || self.h == 0
    }

    pub(crate) fn right(self) -> u32 {
        self.x.saturating_add(self.w)
    }

    pub(crate) fn bottom(self) -> u32 {
        self.y.saturating_add(self.h)
    }

    pub fn intersect(self, o: Rect) -> Rect {
        if self.is_empty() || o.is_empty() {
            return Rect::new(0, 0, 0, 0);
        }
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        if r <= x || b <= y {
            Rect::new(0, 0, 0, 0)
        } else {
            Rect::new(x, y, r - x, b - y)
        }
    }

    pub fn union(self, o: Rect) -> Rect {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return self;
        }
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect::new(
            x,
            y,
            self.right().max(o.right()) - x,
            self.bottom().max(o.bottom()) - y,
        )
    }

    pub(crate) fn area(self) -> u64 {
        self.w as u64 * self.h as u64
    }

    pub(crate) fn overlaps(self, o: Rect) -> bool {
        !self.is_empty()
            && !o.is_empty()
            && self.x < o.right()
            && o.x < self.right()
            && self.y < o.bottom()
            && o.y < self.bottom()
    }
}

#[derive(Clone, Copy)]
pub struct FbInfo {
    pub base: *mut u8,
    pub width: u32,
    pub height: u32,
    pub pitch: u32, // bytes per scanline
    pub format: Format,
}

impl FbInfo {
    pub const fn bpp(self) -> u32 {
        self.format.bpp()
    }

    pub(crate) fn clip(self, r: Rect) -> Option<Rect> {
        let c = r.intersect(Rect::new(0, 0, self.width, self.height));
        if c.is_empty() {
            None
        } else {
            Some(c)
        }
    }

    fn row_addr(self, x: u32, y: u32) -> *mut u8 {
        unsafe {
            self.base
                .add(y as usize * self.pitch as usize + x as usize * self.format.bytes())
        }
    }

    /// Fill `r` with `pixel`; returns the clipped rect actually drawn.
    pub fn fill(self, r: Rect, pixel: u32) -> Option<Rect> {
        let c = self.clip(r)?;
        for y in c.y..c.bottom() {
            let mut p = self.row_addr(c.x, y);
            for _ in c.x..c.right() {
                unsafe { self.format.store(p, pixel) };
                p = unsafe { p.add(self.format.bytes()) };
            }
        }
        Some(c)
    }

    /// XOR `r` with `pixel`; returns the clipped rect (cursor overlay).
    pub fn fill_xor(self, r: Rect, pixel: u32) -> Option<Rect> {
        let c = self.clip(r)?;
        for y in c.y..c.bottom() {
            let mut p = self.row_addr(c.x, y);
            for _ in c.x..c.right() {
                unsafe { self.format.xor(p, pixel) };
                p = unsafe { p.add(self.format.bytes()) };
            }
        }
        Some(c)
    }

    /// Blit same-format source pixels (src_pitch bytes per scanline) into
    /// `dst`; hard-clipped. Returns the clipped rect.
    pub fn blit(self, src: *const u8, src_pitch: u32, dst: Rect) -> Option<Rect> {
        let c = self.clip(dst)?;
        let bpp = self.format.bytes();
        let row = c.w as usize * bpp;
        let srow = unsafe {
            src.add((c.y - dst.y) as usize * src_pitch as usize + (c.x - dst.x) as usize * bpp)
        };
        for dy in 0..c.h {
            unsafe {
                ptr::copy_nonoverlapping(
                    srow.add(dy as usize * src_pitch as usize),
                    self.row_addr(c.x, c.y + dy),
                    row,
                );
            }
        }
        Some(c)
    }

    /// Move a region inside the surface (memmove semantics; overlap-safe).
    /// Returns the clipped destination rect.
    pub fn copy_rect(self, src: Rect, dst: Rect) -> Option<Rect> {
        let c = self.clip(dst)?;
        let bpp = self.format.bytes();
        let row = c.w as usize * bpp;
        let sx = src.x as i64 + (c.x as i64 - dst.x as i64);
        let sy = src.y as i64 + (c.y as i64 - dst.y as i64);
        // Copy against the move direction so a source row is never overwritten
        // before it is read (scroll moves vertically; ptr::copy handles
        // within-row overlap).
        let mut y = 0;
        while y < c.h {
            let ry = if dst.y > src.y { c.h - 1 - y } else { y };
            unsafe {
                let s = self
                    .base
                    .add((sy + ry as i64) as usize * self.pitch as usize + sx as usize * bpp);
                ptr::copy(s, self.row_addr(c.x, c.y + ry), row);
            }
            y += 1;
        }
        Some(c)
    }

    /// Blit an 8x16 1bpp glyph (16 bytes, MSB = leftmost pixel) with fg/bg.
    pub fn blit_glyph(self, dst: Rect, glyph: &[u8; 16], fg: u32, bg: u32) -> Option<Rect> {
        let c = self.clip(dst)?;
        for y in c.y..c.bottom() {
            let bits = glyph[(y - dst.y) as usize];
            for x in c.x..c.right() {
                let on = (bits >> (7 - (x - dst.x))) & 1 == 1;
                unsafe { self.format.store(self.row_addr(x, y), if on { fg } else { bg }) };
            }
        }
        Some(c)
    }
}
