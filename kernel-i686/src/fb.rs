//! VBE linear-framebuffer text console (M10-5, M13-1): character rendering
//! through the shared graphics core (font = blit_glyph, clear/scroll =
//! fill/copy_rect, cursor = XOR fill) with damage marked and a
//! single-buffered present.
//!
//! stage2 sets one VBE linear mode and leaves the geometry, the LFB physical
//! base and a BIOS-copied 8x16 font in BootInfo. The kernel reaches the LFB
//! through the dedicated 0xBFC00000 PSE slot that stage2 maps in every page
//! directory (PDE 767, cloned by user.rs), never through phys_to_virt: the
//! 3G/1G alias stops below 1 GiB while the QEMU LFB sits near 0xFD000000.
//! 16/24/32 bpp are rendered; anything else degrades to the serial console.
//! Serial output is mirrored here byte-for-byte, so the smoke logs are
//! identical with and without a framebuffer.

use core::ptr;

use fantuan_abi::BootInfo;
use kernel_core::graphics::{Damage, DirectPresent, FbInfo, Format, Present, Rect};

use crate::serial;

const FB_SLOT: u32 = 0xBFC0_0000;
const GLYPH_W: u32 = 8;
const GLYPH_H: u32 = 16;
const CURSOR_H: u32 = 2;
/// Light gray; identical in the RGB and BGR channel orders.
const FG: u32 = 0x00D8_D8_D8;
const BG: u32 = 0x0000_0000;

static DIRECT: DirectPresent = DirectPresent;

struct State {
    fb: FbInfo,
    font: *const u8,
    damage: Damage,
    present: &'static dyn Present,
    cols: u32,
    rows: u32,
    row: u32,
    col: u32,
    cursor: bool,
}

static mut STATE: Option<State> = None;

fn state() -> Option<&'static mut State> {
    unsafe { (*ptr::addr_of_mut!(STATE)).as_mut() }
}

/// Validate the stage2 handoff and bring the console up. Returns false (and
/// prints the one fallback line) when VBE or the font is unavailable.
pub fn init(bi: &BootInfo) -> bool {
    let bpp = bi.fb_bpp as u32;
    let width = bi.fb_width as u32;
    let height = bi.fb_height as u32;
    let pitch = bi.fb_pitch;
    let size = pitch as u64 * height as u64;
    let ok = bi.fb_phys != 0
        && bi.fb_phys < 1 << 32
        && bi.fb_font_phys != 0
        && matches!(bpp, 16 | 24 | 32)
        && width >= GLYPH_W
        && height >= GLYPH_H
        && pitch >= width * bpp / 8
        && (bi.fb_phys & 0x3F_FFFF) + size <= 0x40_0000;
    if !ok {
        serial::puts("fb: unavailable (serial console)\n");
        return false;
    }
    let format = match bpp {
        32 => Format::Bpp32,
        24 => Format::Bpp24,
        _ => Format::Bpp16,
    };
    let base = (FB_SLOT + (bi.fb_phys as u32 & 0x3F_FFFF)) as *mut u8;
    let font = crate::phys_to_virt(bi.fb_font_phys as u64) as *const u8;
    unsafe {
        *ptr::addr_of_mut!(STATE) = Some(State {
            fb: FbInfo {
                base,
                width,
                height,
                pitch,
                format,
            },
            font,
            damage: Damage::new(),
            present: &DIRECT,
            cols: width / GLYPH_W,
            rows: height / GLYPH_H,
            row: 0,
            col: 0,
            cursor: false,
        });
    }
    if let Some(st) = state() {
        clear(st);
        present(st);
        cursor_toggle(st, true);
        present(st);
    }
    serial::puts("fb: ");
    serial::put_dec(width as u64);
    serial::puts("x");
    serial::put_dec(height as u64);
    serial::puts("x");
    serial::put_dec(bpp as u64);
    serial::puts(" pitch=");
    serial::put_dec(pitch as u64);
    serial::puts(" at ");
    put_hex_upper(FB_SLOT + (bi.fb_phys as u32 & 0x3F_FFFF));
    serial::puts("\n");
    serial::puts("fb: console up\n");
    true
}

/// Uppercase 0x-prefixed hex, matching the fb: diagnostic convention.
fn put_hex_upper(mut v: u32) {
    serial::puts("0x");
    if v == 0 {
        serial::putc(b'0');
        return;
    }
    let mut buf = [0u8; 8];
    let mut n = 0;
    while v > 0 {
        buf[n] = b"0123456789ABCDEF"[(v & 0xF) as usize];
        v >>= 4;
        n += 1;
    }
    while n > 0 {
        n -= 1;
        serial::putc(buf[n]);
    }
}

/// Serial mirror sink: one byte per call, same stream order as the UART.
pub fn putc(c: u8) {
    let Some(st) = state() else {
        return;
    };
    cursor_toggle(st, false);
    match c {
        b'\n' => {
            st.col = 0;
            newline(st);
        }
        b'\r' => st.col = 0,
        b'\x08' => {
            if st.col > 0 {
                st.col -= 1;
            }
        }
        0x20..=0x7E => {
            draw_glyph(st, c);
            st.col += 1;
            if st.col >= st.cols {
                st.col = 0;
                newline(st);
            }
        }
        _ => {}
    }
    cursor_toggle(st, true);
    present(st);
}

fn present(st: &mut State) {
    st.present.present(&st.fb, &mut st.damage);
}

fn newline(st: &mut State) {
    st.row += 1;
    if st.row >= st.rows {
        scroll(st);
    }
}

fn draw_glyph(st: &mut State, c: u8) {
    let glyph: &[u8; 16] =
        unsafe { &*(st.font.add(c as usize * GLYPH_H as usize) as *const [u8; 16]) };
    let x = st.col * GLYPH_W;
    let y = st.row * GLYPH_H;
    if let Some(r) = st.fb.blit_glyph(Rect::new(x, y, GLYPH_W, GLYPH_H), glyph, FG, BG) {
        st.damage.add(r);
    }
}

fn cursor_toggle(st: &mut State, on: bool) {
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

fn clear(st: &mut State) {
    if let Some(c) = st.fb.fill(Rect::new(0, 0, st.fb.width, st.fb.height), BG) {
        st.damage.add(c);
    }
    st.cursor = false;
}

fn scroll(st: &mut State) {
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
