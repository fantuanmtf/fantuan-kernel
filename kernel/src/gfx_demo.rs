//! Graphics demo task (M13-2): a bounded kernel task that drives the shared
//! framebuffer demo on the GOP console (moving box + counter), reports its
//! per-frame damage accounting on serial through the no-mirror sink (so it
//! never touches the screen), then exits and is reaped by the scheduler.
//! The markers are built into a stack buffer by hand like the M3 demos, since
//! `core::fmt` is avoided on the 16 KiB task stack.

use kernel_core::graphics::Rect;
use kernel_core::task;

const FRAMES: u64 = 40;
const PERIOD_MS: u64 = 100;

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

/// Write `off` bytes of `buf` to serial without mirroring to the GOP.
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

pub fn run() -> ! {
    let mut buf = [0u8; 128];
    let off = push_str(&mut buf, 0, "graphics: demo start\n");
    emit(&buf, off);

    let mut total = Rect::new(0, 0, 0, 0);
    let mut peak_rects = 0usize;
    let mut total_area = 0u64;
    for frame in 0..FRAMES {
        if let Some(stats) = crate::console::gfx_demo_frame(frame) {
            total = total.union(stats.bbox);
            peak_rects = peak_rects.max(stats.rects);
            total_area += stats.area;
            report_frame(&mut buf, frame, stats.bbox);
        }
        task::sleep_ms(PERIOD_MS);
    }

    let idle = crate::console::gfx_demo_idle();
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

    crate::serial::unfreeze_mirror();
    task::exit(0);
}
