//! GOP framebuffer console (M13-1): character rendering through the shared
//! graphics core (font = blit_glyph, clear/scroll = fill/copy_rect, cursor =
//! XOR fill), with damage marked and a single-buffered present. The surface
//! IS the scanout, so `present` clears the damage list; a double-buffered
//! present replaces it in M13-2.
//!
//! v1 policy (DESIGN.md §3): ASCII only, channel-symmetric colors only — the
//! same 32-bit value renders identically in RGB8 and BGR8, so no pixel-format
//! branching is needed for the M0 palette.

use core::fmt;

use fantuan_abi::FrameBuffer;
use kernel_core::graphics::{Damage, DirectPresent, FbInfo, Format, Present, Rect};

use crate::font::FONT_8X16;

const GLYPH_W: u32 = 8;
const GLYPH_H: u32 = 16;
const CURSOR_H: u32 = 2;
const FG: u32 = 0x00D8_D8_D8; // light gray, symmetric in RGB/BGR
const BG: u32 = 0x0000_0000;

static DIRECT: DirectPresent = DirectPresent;

struct State {
    fb: FbInfo,
    damage: Damage,
    present: &'static dyn Present,
    cols: u32,
    rows: u32,
    row: u32,
    col: u32,
    cursor: bool,
}

static mut STATE: Option<State> = None;

pub struct Console;

fn state() -> Option<&'static mut State> {
    unsafe { (*core::ptr::addr_of_mut!(STATE)).as_mut() }
}

impl Console {
    pub fn new(fb: &FrameBuffer) -> Option<Self> {
        if fb.size == 0 || fb.base == 0 || fb.width < GLYPH_W || fb.height < GLYPH_H {
            return None;
        }
        let needed = (fb.stride as u64).checked_mul(fb.height as u64)?.checked_mul(4)?;
        if fb.size < needed {
            return None;
        }
        let format = match fb.format {
            0 | 1 => Format::Bpp32,
            _ => return None,
        };
        let info = FbInfo {
            base: crate::mm::paging::phys_to_virt(fb.base) as *mut u8,
            width: fb.width,
            height: fb.height,
            pitch: fb.stride * 4,
            format,
        };
        unsafe {
            *core::ptr::addr_of_mut!(STATE) = Some(State {
                fb: info,
                damage: Damage::new(),
                present: &DIRECT,
                cols: fb.width / GLYPH_W,
                rows: fb.height / GLYPH_H,
                row: 0,
                col: 0,
                cursor: false,
            });
        }
        if let Some(st) = state() {
            state_clear(st);
            state_present(st);
        }
        Some(Console)
    }
}

/// Mirror one byte from another output channel (serial) onto the console.
pub fn global_putc(c: u8) {
    if let Some(st) = state() {
        state_putc(st, c);
    }
}

impl fmt::Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if let Some(st) = state() {
            for &b in s.as_bytes() {
                state_putc(st, b);
            }
        }
        Ok(())
    }
}

fn state_present(st: &mut State) {
    st.present.present(&st.fb, &mut st.damage);
}

fn state_cursor(st: &mut State, on: bool) {
    if st.cursor == on {
        return;
    }
    st.cursor = on;
    let x = st.col * GLYPH_W;
    let y = st.row * GLYPH_H + (GLYPH_H - CURSOR_H);
    if let Some(c) = st.fb.fill_xor(Rect::new(x, y, GLYPH_W, CURSOR_H), FG) {
        st.damage.add(c);
    }
}

fn state_clear(st: &mut State) {
    if let Some(c) = st.fb.fill(Rect::new(0, 0, st.fb.width, st.fb.height), BG) {
        st.damage.add(c);
    }
    st.row = 0;
    st.col = 0;
    st.cursor = false;
}

fn state_scroll(st: &mut State) {
    let w = st.fb.width;
    let h = st.fb.height;
    let line = GLYPH_H;
    if let Some(c) = st.fb.copy_rect(Rect::new(0, line, w, h - line), Rect::new(0, 0, w, h - line)) {
        st.damage.add(c);
    }
    if let Some(c) = st.fb.fill(Rect::new(0, h - line, w, line), BG) {
        st.damage.add(c);
    }
    st.row -= 1;
}

fn state_newline(st: &mut State) {
    st.col = 0;
    st.row += 1;
    if st.row >= st.rows {
        state_scroll(st);
    }
}

fn state_draw_glyph(st: &mut State, c: u8) {
    let x = st.col * GLYPH_W;
    let y = st.row * GLYPH_H;
    if let Some(r) = st.fb.blit_glyph(
        Rect::new(x, y, GLYPH_W, GLYPH_H),
        &FONT_8X16[c as usize],
        FG,
        BG,
    ) {
        st.damage.add(r);
    }
}

fn state_putc(st: &mut State, c: u8) {
    state_cursor(st, false);
    match c {
        b'\n' => state_newline(st),
        b'\r' => st.col = 0,
        b'\x08' => {
            if st.col > 0 {
                st.col -= 1;
            }
        }
        0x20..=0x7E => {
            state_draw_glyph(st, c);
            st.col += 1;
            if st.col >= st.cols {
                state_newline(st);
            }
        }
        _ => {}
    }
    state_cursor(st, true);
    state_present(st);
}
