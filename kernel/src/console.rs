//! GOP framebuffer console with the embedded Spleen 8x16 font.
//!
//! v1 policy (DESIGN.md §3): ASCII only, channel-symmetric colors only — the
//! same 32-bit value renders identically in RGB8 and BGR8, so no pixel-format
//! branching is needed for the M0 palette.

use core::fmt;

use crate::font::FONT_8X16;
use fantuan_abi::FrameBuffer;

const GLYPH_W: u32 = 8;
const GLYPH_H: u32 = 16;
const FG: u32 = 0x00D8_D8_D8; // light gray, symmetric in RGB/BGR
const BG: u32 = 0x0000_0000;

pub struct Console {
    base: *mut u32,
    stride: u32, // pixels per scanline
    cols: u32,
    rows: u32,
    row: u32,
    col: u32,
}

impl Console {
    pub fn new(fb: &FrameBuffer) -> Option<Self> {
        if fb.size == 0 || fb.base == 0 || fb.width < GLYPH_W || fb.height < GLYPH_H {
            return None;
        }
        // v1 supports RGB8/BGR8 and assumes bit-mask formats are 8-bit/channel.
        match fb.format {
            0 | 1 | 2 => {}
            _ => return None,
        }
        let mut c = Console {
            base: fb.base as *mut u32,
            stride: fb.stride,
            cols: fb.width / GLYPH_W,
            rows: fb.height / GLYPH_H,
            row: 0,
            col: 0,
        };
        c.clear();
        Some(c)
    }

    fn height(&self) -> u32 {
        self.rows * GLYPH_H
    }

    fn put_pixel(&mut self, x: u32, y: u32, color: u32) {
        unsafe {
            *self.base.add((y * self.stride + x) as usize) = color;
        }
    }

    fn draw_glyph(&mut self, c: u8) {
        let bits = &FONT_8X16[c as usize];
        let x0 = self.col * GLYPH_W;
        let y0 = self.row * GLYPH_H;
        for (y, row) in bits.iter().enumerate() {
            for x in 0..8u32 {
                let on = (row >> (7 - x)) & 1 == 1; // MSB = leftmost pixel
                self.put_pixel(x0 + x, y0 + y as u32, if on { FG } else { BG });
            }
        }
    }

    pub fn clear(&mut self) {
        let n = (self.stride * self.height()) as usize;
        unsafe {
            for i in 0..n {
                *self.base.add(i) = BG;
            }
        }
        self.row = 0;
        self.col = 0;
    }

    fn scroll(&mut self) {
        let line = GLYPH_H * self.stride;
        let total = (self.stride * self.height()) as usize;
        unsafe {
            let base = self.base;
            for i in 0..(total - line as usize) {
                *base.add(i) = *base.add(i + line as usize);
            }
            for i in (total - line as usize)..total {
                *base.add(i) = BG;
            }
        }
        self.row -= 1;
    }

    fn newline(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row >= self.rows {
            self.scroll();
        }
    }

    pub fn putc(&mut self, c: u8) {
        match c {
            b'\n' => self.newline(),
            b'\r' => self.col = 0,
            b'\x08' => {
                if self.col > 0 {
                    self.col -= 1;
                }
            }
            0x20..=0x7E => {
                self.draw_glyph(c);
                self.col += 1;
                if self.col >= self.cols {
                    self.newline();
                }
            }
            _ => {}
        }
    }
}

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for &b in s.as_bytes() {
            self.putc(b);
        }
        Ok(())
    }
}
