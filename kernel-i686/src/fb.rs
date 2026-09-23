//! VBE linear-framebuffer text console (M10-5, M13-1/M13-2): character
//! rendering through the shared graphics core (font = blit_glyph, clear/scroll
//! = fill/copy_rect, cursor = XOR fill) with damage marked. The console starts
//! single-buffered (the render surface IS the scanout); after the frame
//! allocator is up, `upgrade_buffered` allocates an off-screen frame and swaps
//! in a `BufferedPresent` that copies only the damaged rects to the scanout.
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
use kernel_core::graphics::{
    demo_cursor, demo_frame, BufferedPresent, Damage, DemoStats, DirectPresent, FbInfo, Format,
    Present, Rect, CURSOR_SIZE,
};

use crate::serial;

const FB_SLOT: u32 = 0xBFC0_0000;
const GLYPH_W: u32 = 8;
const GLYPH_H: u32 = 16;
const CURSOR_H: u32 = 2;
/// Light gray; identical in the RGB and BGR channel orders.
const FG: u32 = 0x00D8_D8_D8;
const BG: u32 = 0x0000_0000;

static DIRECT: DirectPresent = DirectPresent;
static mut BUFFERED: Option<BufferedPresent> = None;

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
    let device = FbInfo {
        base,
        width,
        height,
        pitch,
        format,
    };
    unsafe {
        *ptr::addr_of_mut!(STATE) = Some(State {
            fb: device,
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

/// M13-2: switch the console from direct to double-buffered presentation by
/// allocating an off-screen frame from the frame allocator (bounded to the
/// mode's exact size), copying the current scanout into it, and swapping the
/// render surface + `BufferedPresent`. On failure the console keeps
/// `DirectPresent`. No-op when the console is absent (serial-only boot).
pub fn upgrade_buffered() {
    let Some(st) = state() else {
        return;
    };
    let w = st.fb.width;
    let h = st.fb.height;
    let bytes = (st.fb.bpp() / 8) as u64;
    let needed = w as u64 * h as u64 * bytes;
    let frames = ((needed + 4095) / 4096) as usize;
    let Some(phys) = kernel_core::frame::get().alloc_contiguous(frames) else {
        serial::puts("fb: direct present (back buffer unavailable)\n");
        return;
    };
    let back = FbInfo {
        base: crate::phys_to_virt(phys) as *mut u8,
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
    serial::puts("fb: double-buffered (frame-allocator back buffer)\n");
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

/// One graphics-demo frame on the shared off-screen surface (box + counter),
/// accumulating damage without presenting.
pub fn gfx_demo_frame(frame: u64) -> Option<DemoStats> {
    let st = state()?;
    Some(demo_frame(st.fb, &mut st.damage, frame))
}

/// Erase the cursor at its old position and draw it at (x, y); union rect.
pub fn gfx_demo_cursor(old_x: u32, old_y: u32, x: u32, y: u32) -> Option<Rect> {
    let st = state()?;
    let mut bbox = Rect::new(0, 0, 0, 0);
    if let Some(c) = st.fb.fill(Rect::new(old_x, old_y, CURSOR_SIZE, CURSOR_SIZE), BG) {
        st.damage.add(c);
        bbox = bbox.union(c);
    }
    if let Some(c) = demo_cursor(st.fb, &mut st.damage, x, y) {
        bbox = bbox.union(c);
    }
    Some(bbox)
}

/// Present the demo's accumulated damage (one present per frame).
pub fn gfx_demo_present() {
    if let Some(st) = state() {
        present(st);
    }
}

/// Framebuffer dimensions for the demo's cursor clamping.
pub fn gfx_demo_dims() -> Option<(u32, u32)> {
    state().map(|st| (st.fb.width, st.fb.height))
}

/// True when the shared damage list is empty (idle-screen proof).
pub fn gfx_demo_idle() -> bool {
    state().map_or(true, |st| st.damage.is_empty())
}
