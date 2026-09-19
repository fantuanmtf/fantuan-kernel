//! VBE linear-framebuffer text console (M10-5).
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

use crate::serial;

const FB_SLOT: u32 = 0xBFC0_0000;
const GLYPH_W: u32 = 8;
const GLYPH_H: u32 = 16;
const CURSOR_H: u32 = 2;
/// Light gray; identical in the RGB and BGR channel orders.
const FG: u32 = 0x00D8_D8_D8;
const BG: u32 = 0x0000_0000;

struct State {
    base: *mut u8,
    pitch: u32,
    height: u32,
    bpp: u32,
    font: *const u8,
    cols: u32,
    rows: u32,
    row: u32,
    col: u32,
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
    let base = (FB_SLOT + (bi.fb_phys as u32 & 0x3F_FFFF)) as *mut u8;
    let font = crate::phys_to_virt(bi.fb_font_phys as u64) as *const u8;
    unsafe {
        *ptr::addr_of_mut!(STATE) = Some(State {
            base,
            pitch,
            height,
            bpp,
            font,
            cols: width / GLYPH_W,
            rows: height / GLYPH_H,
            row: 0,
            col: 0,
        });
    }
    if let Some(st) = state() {
        clear(st);
        cursor_toggle(st);
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
    // The cursor is an XOR overlay, so toggling it off restores whatever was
    // underneath (a glyph drawn earlier, or blank); toggling again draws it at
    // the new position without destructive erasing.
    cursor_toggle(st);
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
    cursor_toggle(st);
}

fn newline(st: &mut State) {
    st.row += 1;
    if st.row >= st.rows {
        scroll(st);
    }
}

fn put_pixel(st: &State, x: u32, y: u32, color: u32) {
    let off = y * st.pitch + x * st.bpp / 8;
    unsafe {
        let p = st.base.add(off as usize);
        match st.bpp {
            32 => (p as *mut u32).write_volatile(color),
            24 => {
                p.write((color & 0xFF) as u8);
                p.add(1).write(((color >> 8) & 0xFF) as u8);
                p.add(2).write(((color >> 16) & 0xFF) as u8);
            }
            16 => {
                let v = (((color >> 19) & 0x1F) << 11)
                    | (((color >> 10) & 0x3F) << 5)
                    | ((color >> 3) & 0x1F);
                (p as *mut u16).write_volatile(v as u16);
            }
            _ => {}
        }
    }
}

fn draw_glyph(st: &State, c: u8) {
    let glyph = unsafe { st.font.add(c as usize * GLYPH_H as usize) };
    let x0 = st.col * GLYPH_W;
    let y0 = st.row * GLYPH_H;
    for y in 0..GLYPH_H {
        let bits = unsafe { glyph.add(y as usize).read() };
        for x in 0..GLYPH_W {
            let on = (bits >> (7 - x)) & 1 == 1;
            put_pixel(st, x0 + x, y0 + y, if on { FG } else { BG });
        }
    }
}

fn toggle_pixel(st: &State, x: u32, y: u32, color: u32) {
    let off = y * st.pitch + x * st.bpp / 8;
    unsafe {
        let p = st.base.add(off as usize);
        match st.bpp {
            32 => (p as *mut u32).write_volatile((p as *mut u32).read_volatile() ^ color),
            24 => {
                p.write(p.read() ^ (color & 0xFF) as u8);
                p.add(1).write(p.add(1).read() ^ ((color >> 8) & 0xFF) as u8);
                p.add(2).write(p.add(2).read() ^ ((color >> 16) & 0xFF) as u8);
            }
            16 => {
                let v = (((color >> 19) & 0x1F) << 11)
                    | (((color >> 10) & 0x3F) << 5)
                    | ((color >> 3) & 0x1F);
                (p as *mut u16).write_volatile((p as *mut u16).read_volatile() ^ v as u16);
            }
            _ => {}
        }
    }
}

fn cursor_toggle(st: &State) {
    let x0 = st.col * GLYPH_W;
    let y0 = st.row * GLYPH_H;
    for y in (GLYPH_H - CURSOR_H)..GLYPH_H {
        for x in 0..GLYPH_W {
            toggle_pixel(st, x0 + x, y0 + y, FG);
        }
    }
}

fn clear(st: &State) {
    let total = st.pitch as usize * st.height as usize;
    unsafe { ptr::write_bytes(st.base, 0, total) };
}

fn scroll(st: &mut State) {
    let line = GLYPH_H * st.pitch;
    let total = st.height * st.pitch;
    unsafe {
        ptr::copy(st.base.add(line as usize), st.base, total as usize - line as usize);
        ptr::write_bytes(st.base.add((total - line) as usize), 0, line as usize);
    }
    st.row -= 1;
}
