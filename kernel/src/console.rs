//! GOP framebuffer console (M13-1/M13-2): character rendering through the
//! shared graphics core (font = blit_glyph, clear/scroll = fill/copy_rect,
//! cursor = XOR fill) with damage marked. The console starts single-buffered
//! (the render surface IS the scanout); after the frame allocator is up,
//! `upgrade_buffered` allocates an off-screen frame and swaps in a
//! `BufferedPresent` that copies only the damaged rects to the scanout. If the
//! allocation fails the console keeps `DirectPresent`.
//!
//! v1 policy (DESIGN.md §3): ASCII only, channel-symmetric colors only — the
//! same 32-bit value renders identically in RGB8 and BGR8, so no pixel-format
//! branching is needed for the M0 palette.

use core::fmt;
use core::ptr;

use fantuan_abi::FrameBuffer;
use kernel_core::graphics::{
    demo_frame, BufferedPresent, Damage, DirectPresent, DemoStats, FbInfo, Format, Present, Rect,
};

use crate::font::FONT_8X16;

const GLYPH_W: u32 = 8;
const GLYPH_H: u32 = 16;
const CURSOR_H: u32 = 2;
const FG: u32 = 0x00D8_D8_D8; // light gray, symmetric in RGB/BGR
const BG: u32 = 0x0000_0000;

static DIRECT: DirectPresent = DirectPresent;
static mut BUFFERED: Option<BufferedPresent> = None;

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

/// M13-2: switch the console from direct to double-buffered presentation by
/// allocating an off-screen frame from the frame allocator (bounded to the
/// mode's exact size), copying the current scanout into it, and swapping the
/// render surface + `BufferedPresent`. Returns false (leaving `DirectPresent`
/// in place) when the allocation fails or the console is absent.
pub(crate) fn upgrade_buffered() -> bool {
    let Some(st) = state() else {
        return false;
    };
    let w = st.fb.width;
    let h = st.fb.height;
    let bytes = (st.fb.bpp() / 8) as u64;
    let needed = w as u64 * h as u64 * bytes;
    let frames = ((needed + 4095) / 4096) as usize;
    let Some(phys) = crate::mm::frame::get().alloc_contiguous(frames) else {
        return false;
    };
    let back = FbInfo {
        base: crate::mm::paging::phys_to_virt(phys) as *mut u8,
        width: w,
        height: h,
        pitch: w * (st.fb.bpp() / 8),
        format: st.fb.format,
    };
    // Preserve the boot banner already rendered directly to the scanout.
    let _ = back.blit(st.fb.base, st.fb.pitch, Rect::new(0, 0, w, h));
    unsafe {
        ptr::write(ptr::addr_of_mut!(BUFFERED), Some(BufferedPresent::new(st.fb)));
    }
    let p: &'static dyn Present = unsafe { (*ptr::addr_of!(BUFFERED)).as_ref().unwrap() };
    st.fb = back;
    st.present = p;
    true
}

/// One graphics-demo frame on the shared off-screen surface, then present.
/// Returns the frame's damage summary (`None` when the console is absent).
pub(crate) fn gfx_demo_frame(frame: u64) -> Option<DemoStats> {
    let st = state()?;
    let stats = demo_frame(st.fb, &mut st.damage, frame);
    state_present(st);
    Some(stats)
}

/// True when the shared damage list is empty (idle-screen proof).
pub(crate) fn gfx_demo_idle() -> bool {
    state().map_or(true, |st| st.damage.is_empty())
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
