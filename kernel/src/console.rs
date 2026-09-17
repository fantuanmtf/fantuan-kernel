//! GOP framebuffer console with the embedded Spleen 8x16 font.
//!
//! v1 policy (DESIGN.md §3): ASCII only, channel-symmetric colors only — the
//! same 32-bit value renders identically in RGB8 and BGR8, so no pixel-format
//! branching is needed for the M0 palette.
//!
//! M8.5b: the console lives in one global state. `Console::new` initializes
//! it; the facade methods, the fmt::Write impl and the serial mirror
//! (`serial::enable_mirror` -> global_putc) all write through the same
//! cursor, so shell output is visible on the GOP even without a serial port.

use core::fmt;

use crate::font::FONT_8X16;
use fantuan_abi::FrameBuffer;

const GLYPH_W: u32 = 8;
const GLYPH_H: u32 = 16;
const FG: u32 = 0x00D8_D8_D8; // light gray, symmetric in RGB/BGR
const BG: u32 = 0x0000_0000;

struct State {
    base: *mut u32,
    stride: u32, // pixels per scanline
    cols: u32,
    rows: u32,
    row: u32,
    col: u32,
}

static mut STATE: Option<State> = None;

/// The console handle. `new` validates the framebuffer and initializes the
/// single global console; a returned handle is only a marker (all state is
/// global so the serial mirror can share the cursor).
pub struct Console;

fn state() -> Option<&'static mut State> {
    unsafe { (*core::ptr::addr_of_mut!(STATE)).as_mut() }
}

impl Console {
    pub fn new(fb: &FrameBuffer) -> Option<Self> {
        if fb.size == 0 || fb.base == 0 || fb.width < GLYPH_W || fb.height < GLYPH_H {
            return None;
        }
        // The backing store must cover every pixel we address; a QEMU or
        // firmware bug that reports a short buffer must not turn into wild
        // writes. 32 bpp only.
        let needed = (fb.stride as u64).checked_mul(fb.height as u64)?.checked_mul(4)?;
        if fb.size < needed {
            return None;
        }
        // v1 supports RGB8/BGR8 only (channel-symmetric colors); bit-mask
        // formats need per-channel shifts and are rejected rather than
        // rendered incorrectly.
        match fb.format {
            0 | 1 => {}
            _ => return None,
        }
        unsafe {
            *core::ptr::addr_of_mut!(STATE) = Some(State {
                // Access the framebuffer through the PHYS_OFFSET alias, not
                // the identity-mapped physical address: the alias exists in
                // every address space (user tasks clone PML4 entry 256),
                // while the identity map does not. With the serial mirror
                // (M8.5b), a user task's sys_write can reach this code.
                base: crate::mm::paging::phys_to_virt(fb.base) as *mut u32,
                stride: fb.stride,
                cols: fb.width / GLYPH_W,
                rows: fb.height / GLYPH_H,
                row: 0,
                col: 0,
            });
        }
        if let Some(st) = state() {
            state_clear(st);
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

// --- state operations ------------------------------------------------------

fn state_height(st: &State) -> u32 {
    st.rows * GLYPH_H
}

fn state_put_pixel(st: &mut State, x: u32, y: u32, color: u32) {
    unsafe {
        *st.base.add((y * st.stride + x) as usize) = color;
    }
}

fn state_draw_glyph(st: &mut State, c: u8) {
    let bits = &FONT_8X16[c as usize];
    let x0 = st.col * GLYPH_W;
    let y0 = st.row * GLYPH_H;
    for (y, row) in bits.iter().enumerate() {
        for x in 0..8u32 {
            let on = (row >> (7 - x)) & 1 == 1; // MSB = leftmost pixel
            state_put_pixel(st, x0 + x, y0 + y as u32, if on { FG } else { BG });
        }
    }
}

fn state_clear(st: &mut State) {
    let n = (st.stride * state_height(st)) as usize;
    unsafe {
        for i in 0..n {
            *st.base.add(i) = BG;
        }
    }
    st.row = 0;
    st.col = 0;
}

fn state_scroll(st: &mut State) {
    let line = GLYPH_H * st.stride;
    let total = (st.stride * state_height(st)) as usize;
    unsafe {
        let base = st.base;
        for i in 0..(total - line as usize) {
            *base.add(i) = *base.add(i + line as usize);
        }
        for i in (total - line as usize)..total {
            *base.add(i) = BG;
        }
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

fn state_putc(st: &mut State, c: u8) {
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
}
