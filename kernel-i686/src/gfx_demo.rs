//! Graphics demo task (M13-2/M13-3, i686): a bounded kernel task that drives
//! the shared framebuffer demo on the VBE console (moving box + counter + a
//! pointer-driven cursor sprite), reports its per-frame damage accounting on
//! serial through the no-mirror sink, and consumes the input event ring:
//! pointer events move the cursor and a `q`/`Esc` key press stops the
//! animation early. Markers are built into a stack buffer by hand (no
//! `core::fmt` on the 16 KiB task stack). On a headless boot the console access
//! returns `None` and the task just prints its markers (serial parity holds).

use kernel_core::graphics::Rect;
use kernel_core::task;

const FRAMES: u64 = 80;
const PERIOD_MS: u64 = 100;
const CURSOR_HOME_X: u32 = 16;
const ESC_SCANCODE: u8 = 1;

fn push_str(buf: &mut [u8], mut off: usize, s: &str) -> usize {
    for &b in s.as_bytes() {
        if off < buf.len() {
            buf[off] = b;
            off += 1;
        }
    }
    off
}

fn push_u64(buf: &mut [u8], mut off: usize, mut v: u64) -> usize {
    if v == 0 {
        buf[off] = b'0';
        return off + 1;
    }
    let mut tmp = [0u8; 20];
    let mut n = 0;
    while v > 0 {
        tmp[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
    }
    while n > 0 {
        n -= 1;
        buf[off] = tmp[n];
        off += 1;
    }
    off
}

fn push_i64(buf: &mut [u8], mut off: usize, v: i64) -> usize {
    if v < 0 {
        buf[off] = b'-';
        off += 1;
        return push_u64(buf, off, (v as i64).unsigned_abs());
    }
    push_u64(buf, off, v as u64)
}

fn emit(buf: &[u8], off: usize) {
    crate::serial::log_bytes_raw(&buf[..off]);
}

fn report_frame(buf: &mut [u8], frame: u64, r: Rect) {
    let mut off = 0;
    off = push_str(buf, off, "graphics: demo frame=");
    off = push_u64(buf, off, frame);
    off = push_str(buf, off, " dmg=");
    off = push_u64(buf, off, r.x as u64);
    off = push_str(buf, off, ",");
    off = push_u64(buf, off, r.y as u64);
    off = push_str(buf, off, ",");
    off = push_u64(buf, off, r.w as u64);
    off = push_str(buf, off, ",");
    off = push_u64(buf, off, r.h as u64);
    buf[off] = b'\n';
    emit(buf, off + 1);
}

fn report_pointer(buf: &mut [u8], dx: i64, dy: i64, buttons: u8) {
    let mut off = 0;
    off = push_str(buf, off, "input: demo consumed dx=");
    off = push_i64(buf, off, dx);
    off = push_str(buf, off, " dy=");
    off = push_i64(buf, off, dy);
    off = push_str(buf, off, " buttons=");
    off = push_u64(buf, off, buttons as u64);
    buf[off] = b'\n';
    emit(buf, off + 1);
}

fn report_cursor(buf: &mut [u8], x: u32, y: u32) {
    let mut off = 0;
    off = push_str(buf, off, "input: demo cursor x=");
    off = push_u64(buf, off, x as u64);
    off = push_str(buf, off, " y=");
    off = push_u64(buf, off, y as u64);
    buf[off] = b'\n';
    emit(buf, off + 1);
}

fn report_key_stop(buf: &mut [u8], scancode: u8, ascii: u8) {
    let mut off = 0;
    off = push_str(buf, off, "input: demo key stop scancode=");
    off = push_u64(buf, off, scancode as u64);
    off = push_str(buf, off, " ascii=");
    off = push_u64(buf, off, ascii as u64);
    buf[off] = b'\n';
    emit(buf, off + 1);
}

pub fn run() -> ! {
    let mut buf = [0u8; 128];
    let off = push_str(&mut buf, 0, "graphics: demo start\n");
    emit(&buf, off);

    let (w, h) = crate::fb::gfx_demo_dims().unwrap_or((1024, 768));
    let clamp_x = |v: i32| v.clamp(0, w as i32 - kernel_core::graphics::CURSOR_SIZE as i32) as u32;
    let clamp_y = |v: i32| v.clamp(0, h as i32 - kernel_core::graphics::CURSOR_SIZE as i32) as u32;
    let mut cx = CURSOR_HOME_X;
    let mut cy = h.saturating_sub(96); // top of the demo's bottom band
    let mut old_cx = cx;
    let mut old_cy = cy;

    let mut total = Rect::new(0, 0, 0, 0);
    let mut peak_rects = 0usize;
    let mut total_area = 0u64;
    let mut stopped = false;
    let mut stop_sc = 0u8;
    let mut stop_ascii = 0u8;

    for frame in 0..FRAMES {
        loop {
            match kernel_core::input_ring::pop() {
                Some(ev) if ev.kind == kernel_core::input_ring::kind::POINTER => {
                    let nx = clamp_x(cx as i32 + ev.dx as i32);
                    let ny = clamp_y(cy as i32 + ev.dy as i32);
                    cx = nx;
                    cy = ny;
                    report_pointer(&mut buf, ev.dx as i64, ev.dy as i64, ev.buttons);
                    report_cursor(&mut buf, cx, cy);
                }
                Some(ev)
                    if ev.kind == kernel_core::input_ring::kind::KEY
                        && ev.flags == kernel_core::input_ring::KEY_PRESSED
                        && (ev.ascii == b'q' || ev.ascii == b'Q' || ev.scancode == ESC_SCANCODE) =>
                {
                    stopped = true;
                    stop_sc = ev.scancode;
                    stop_ascii = ev.ascii;
                    break;
                }
                Some(_) => {}
                None => break,
            }
        }

        let stats = crate::fb::gfx_demo_frame(frame);
        let cur = crate::fb::gfx_demo_cursor(old_cx, old_cy, cx, cy);
        crate::fb::gfx_demo_present();
        match stats {
            Some(s) => {
                let bbox = cur.map_or(s.bbox, |c| s.bbox.union(c));
                total = total.union(bbox);
                peak_rects = peak_rects.max(s.rects);
                total_area += s.area;
                report_frame(&mut buf, frame, bbox);
            }
            None => {
                if let Some(c) = cur {
                    total = total.union(c);
                }
            }
        }
        old_cx = cx;
        old_cy = cy;
        if stopped {
            break;
        }
        task::sleep_ms(PERIOD_MS);
    }

    if stopped {
        report_key_stop(&mut buf, stop_sc, stop_ascii);
    }

    let idle = crate::fb::gfx_demo_idle();
    let mut off = push_str(&mut buf, 0, "graphics: demo stop\n");
    emit(&buf, off);
    off = push_str(&mut buf, 0, "graphics: demo damage total=");
    off = push_u64(&mut buf, off, total.x as u64);
    off = push_str(&mut buf, off, ",");
    off = push_u64(&mut buf, off, total.y as u64);
    off = push_str(&mut buf, off, ",");
    off = push_u64(&mut buf, off, total.w as u64);
    off = push_str(&mut buf, off, ",");
    off = push_u64(&mut buf, off, total.h as u64);
    off = push_str(&mut buf, off, " rects=");
    off = push_u64(&mut buf, off, peak_rects as u64);
    off = push_str(&mut buf, off, " area=");
    off = push_u64(&mut buf, off, total_area);
    buf[off] = b'\n';
    emit(&buf, off + 1);
    off = push_str(
        &mut buf,
        0,
        if idle {
            "graphics: idle damage empty ok\n"
        } else {
            "graphics: idle damage FAILED\n"
        },
    );
    emit(&buf, off);

    task::exit(0);
}
