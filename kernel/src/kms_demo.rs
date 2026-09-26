//! KMS demo task (M13-4, x86_64): proves the dumb-buffer pool and the
//! ADDFB/SETCRTC/PAGE_FLIP contract on the GOP console. It allocates two dumb
//! buffers, registers both as framebuffers, SETCRTCs the first, then flips
//! between them for FLIPS frames while consuming the flip-complete events,
//! runs a negative geometry-mismatch SETCRTC, frees everything and reports
//! the allocator accounting. Markers are built into a stack buffer by hand
//! (no `core::fmt` on the 16 KiB task stack) and written over the no-mirror
//! sink so the demo never draws its own text.

use kernel_core::graphics::{event, kms, Damage, FbInfo};
use kernel_core::task;

const FLIPS: u64 = 8;
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

fn emit(buf: &[u8], off: usize) {
    crate::serial::log_bytes_raw(&buf[..off]);
}

fn report_addfb(buf: &mut [u8], id: u32, w: u32, h: u32) {
    let mut off = 0;
    off = push_str(buf, off, "gfx: addfb id=");
    off = push_u64(buf, off, id as u64);
    off = push_str(buf, off, " w=");
    off = push_u64(buf, off, w as u64);
    off = push_str(buf, off, " h=");
    off = push_u64(buf, off, h as u64);
    buf[off] = b'\n';
    emit(buf, off + 1);
}

fn report_setcrtc(buf: &mut [u8], id: u32) {
    let mut off = 0;
    off = push_str(buf, off, "gfx: setcrtc fb=");
    off = push_u64(buf, off, id as u64);
    buf[off] = b'\n';
    emit(buf, off + 1);
}

fn report_flip(buf: &mut [u8], id: u32, seq: u32) {
    let mut off = 0;
    off = push_str(buf, off, "gfx: page_flip fb=");
    off = push_u64(buf, off, id as u64);
    off = push_str(buf, off, " event=");
    off = push_u64(buf, off, seq as u64);
    buf[off] = b'\n';
    emit(buf, off + 1);
}

fn report_loop(buf: &mut [u8], frames: u64) {
    let mut off = 0;
    off = push_str(buf, off, "gfx: flip loop ok frames=");
    off = push_u64(buf, off, frames);
    buf[off] = b'\n';
    emit(buf, off + 1);
}

fn report_fail(buf: &mut [u8], msg: &str) {
    let mut off = 0;
    off = push_str(buf, off, "gfx: ");
    off = push_str(buf, off, msg);
    buf[off] = b'\n';
    emit(buf, off + 1);
}

/// Render one frame of animation into a dumb buffer's surface. The buffer is
/// cleared first so the untouched area stays a known background (the frame
/// allocator hands back uninitialized pages).
fn render(fb: FbInfo, frame: u64) {
    let _ = fb.fill(kernel_core::graphics::Rect::new(0, 0, fb.width, fb.height), 0);
    let mut d = Damage::new();
    let _ = kernel_core::graphics::demo_frame(fb, &mut d, frame);
}

/// Abort after the dumb buffers exist: free them, hand the console surface
/// back and unfreeze the mirror before exiting.
fn bail(buf: &mut [u8], msg: &str, h1: u32, h2: u32) -> ! {
    report_fail(buf, msg);
    let _ = kms::dumb_destroy(h1);
    let _ = kms::dumb_destroy(h2);
    crate::console::gfx_restore();
    crate::serial::unfreeze_mirror();
    task::exit(0)
}

pub fn run() -> ! {
    let mut buf = [0u8; 160];

    let Some(dev) = kms::scanout() else {
        report_fail(&mut buf, "no scanout");
        crate::serial::unfreeze_mirror();
        task::exit(0);
    };
    let w = dev.width;
    let h = dev.height;
    let format = dev.format;
    let pitch = w * (format.bpp() / 8);
    let baseline = kernel_core::frame::get().usable_mib();

    let Some(h1) = kms::dumb_create(w, h, format) else {
        report_fail(&mut buf, "dumb_create FAILED");
        crate::serial::unfreeze_mirror();
        task::exit(0);
    };
    let Some(h2) = kms::dumb_create(w, h, format) else {
        report_fail(&mut buf, "dumb_create FAILED");
        let _ = kms::dumb_destroy(h1);
        crate::serial::unfreeze_mirror();
        task::exit(0);
    };
    let Some(id1) = kms::addfb(h1, w, h, pitch, format) else {
        report_fail(&mut buf, "addfb FAILED");
        let _ = kms::dumb_destroy(h1);
        let _ = kms::dumb_destroy(h2);
        crate::serial::unfreeze_mirror();
        task::exit(0);
    };
    let Some(id2) = kms::addfb(h2, w, h, pitch, format) else {
        report_fail(&mut buf, "addfb FAILED");
        let _ = kms::dumb_destroy(h1);
        let _ = kms::dumb_destroy(h2);
        crate::serial::unfreeze_mirror();
        task::exit(0);
    };
    report_addfb(&mut buf, id1, w, h);
    report_addfb(&mut buf, id2, w, h);

    let Some(fb1) = kms::dumb_info(h1) else { bail(&mut buf, "dumb_info FAILED", h1, h2) };
    let Some(fb2) = kms::dumb_info(h2) else { bail(&mut buf, "dumb_info FAILED", h1, h2) };
    render(fb1, 0);
    render(fb2, 1);

    if kms::setcrtc(id1).is_ok() {
        report_setcrtc(&mut buf, id1);
    } else {
        report_fail(&mut buf, "setcrtc FAILED");
    }

    // The two buffers alternate: even iterations present h2, odd ones h1
    // (the CRTC starts on id1, so the first flip goes to the other buffer).
    let mut frames = 0u64;
    for i in 0..FLIPS {
        let (fb_id, handle) = if i % 2 == 0 { (id2, h2) } else { (id1, h1) };
        let Some(off_fb) = kms::dumb_info(handle) else {
            bail(&mut buf, "dumb_info FAILED", h1, h2)
        };
        render(off_fb, i + 2);
        if kms::page_flip(fb_id).is_ok() {
            match event::pop() {
                Some(ev) if ev.kind == event::kind::FLIP_COMPLETE => {
                    report_flip(&mut buf, fb_id, ev.seq);
                }
                _ => report_fail(&mut buf, "flip event missing"),
            }
        } else {
            report_fail(&mut buf, "page_flip FAILED");
        }
        frames += 1;
        task::sleep_ms(PERIOD_MS);
    }
    report_loop(&mut buf, frames);

    // Negative test: an FB whose geometry does not match the CRTC mode.
    let Some(id3) = kms::addfb(h1, 64, 64, 64 * (format.bpp() / 8), format) else {
        report_fail(&mut buf, "addfb (mismatch) FAILED");
        let _ = kms::rmfb(id1);
        let _ = kms::rmfb(id2);
        let _ = kms::dumb_destroy(h1);
        let _ = kms::dumb_destroy(h2);
        crate::console::gfx_restore();
        crate::serial::unfreeze_mirror();
        task::exit(0);
    };
    match kms::setcrtc(id3) {
        Err(kms::KmsError::GeometryMismatch) => {
            let _ = event::pop();
            let off = push_str(&mut buf, 0, "gfx: setcrtc mismatch ok\n");
            emit(&buf, off);
        }
        _ => report_fail(&mut buf, "setcrtc mismatch FAILED"),
    }
    let _ = kms::rmfb(id3);

    let _ = kms::rmfb(id1);
    let _ = kms::rmfb(id2);
    let _ = kms::dumb_destroy(h1);
    let _ = kms::dumb_destroy(h2);
    let used = kms::dumb_used();
    let now = kernel_core::frame::get().usable_mib();
    if used == 0 && now == baseline {
        let off = push_str(&mut buf, 0, "gfx: cleanup ok\n");
        emit(&buf, off);
    } else {
        let mut off = push_str(&mut buf, 0, "gfx: cleanup FAILED used=");
        off = push_u64(&mut buf, off, used as u64);
        off = push_str(&mut buf, off, " usable=");
        off = push_u64(&mut buf, off, now);
        off = push_str(&mut buf, off, " baseline=");
        off = push_u64(&mut buf, off, baseline);
        buf[off] = b'\n';
        emit(&buf, off + 1);
    }

    crate::console::gfx_restore();
    crate::serial::unfreeze_mirror();
    task::exit(0);
}
