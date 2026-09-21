//! P2 console line discipline (docs/POSIX_PLAN.md): canonical input over the
//! merged serial+PS/2 byte source with echo, erase, EOF (^D) and ISIG (^C).
//!
//! `read` blocks by polling `crate::input::poll_byte()` and sleeping one tick
//! between polls; the completed line is appended to a pending buffer that
//! `read` drains. The kernel's own shell keeps reading `input` directly, so
//! only one of the two readers is active at a time (the shell blocks in
//! wait4 while a user shell owns the console).

use core::sync::atomic::AtomicBool;

use fantuan_abi::{SYS_ERR_NOTTY, Termios};

use crate::arch::IrqLock;
use crate::task;

/// termios flag bits (mirror libc-fantuan/termios.h; octal in the header).
const ICRNL: u32 = 0o400;
const OPOST: u32 = 0o1;
const CS8: u32 = 0o60;
const CREAD: u32 = 0o200;
const ISIG: u32 = 0o1;
const ICANON: u32 = 0o2;
const ECHO: u32 = 0o10;
const ECHOE: u32 = 0o20;
const ECHONL: u32 = 0o100;
const IXON: u32 = 0o2000;

// c_cc indices (mirror libc-fantuan/termios.h).
const VINTR: usize = 0;
const VQUIT: usize = 1;
const VERASE: usize = 2;
const VKILL: usize = 3;
const VEOF: usize = 4;

const PEND_MAX: usize = 512;
const LINE_MAX: usize = 256;

static TTY_LOCK: AtomicBool = AtomicBool::new(false);

struct State {
    iflag: u32,
    oflag: u32,
    cflag: u32,
    lflag: u32,
    cc: [u8; 32],
    line: [u8; LINE_MAX],
    line_len: usize,
    pend: [u8; PEND_MAX],
    pend_len: usize,
    eof_once: bool,
}

static mut S: State = State {
    iflag: ICRNL,
    oflag: OPOST,
    cflag: CS8 | CREAD,
    lflag: ISIG | ICANON | ECHO,
    cc: [0; 32],
    line: [0; LINE_MAX],
    line_len: 0,
    pend: [0; PEND_MAX],
    pend_len: 0,
    eof_once: false,
};

fn st() -> &'static mut State {
    unsafe { &mut *core::ptr::addr_of_mut!(S) }
}

fn lock() -> IrqLock {
    IrqLock::acquire(&TTY_LOCK)
}

/// One-time defaults: VINTR ^C, VQUIT ^\, VERASE DEL, VKILL ^U, VEOF ^D.
fn ensure_defaults() {
    let s = st();
    if s.cc[VINTR] == 0 {
        s.cc[VINTR] = 3;
        s.cc[VQUIT] = 0x1C;
        s.cc[VERASE] = 0x7F;
        s.cc[VKILL] = 0x15;
        s.cc[VEOF] = 4;
        s.cc[7] = 0x13; // VSTOP ^S
        s.cc[8] = 0x11; // VSTART ^Q
        s.cc[10] = 0x1A; // VSUSP ^Z
    }
}

pub fn termios_get() -> Termios {
    ensure_defaults();
    let s = st();
    Termios { c_iflag: s.iflag, c_oflag: s.oflag, c_cflag: s.cflag, c_lflag: s.lflag, c_cc: s.cc }
}

pub fn termios_set(t: &Termios) -> u64 {
    ensure_defaults();
    let s = st();
    s.iflag = t.c_iflag;
    s.oflag = t.c_oflag;
    s.cflag = t.c_cflag;
    s.lflag = t.c_lflag;
    s.cc = t.c_cc;
    fantuan_abi::SYS_OK
}

pub fn flush() {
    let _g = lock();
    let s = st();
    s.pend_len = 0;
    s.line_len = 0;
    s.eof_once = false;
}

fn echo(bytes: &[u8]) {
    if st().lflag & ECHO != 0 {
        crate::log::put(bytes);
    }
}

fn push_pend(s: &mut State, b: u8) {
    if s.pend_len < PEND_MAX {
        s.pend[s.pend_len] = b;
        s.pend_len += 1;
    }
}

fn complete_line(s: &mut State, newline: bool) {
    for i in 0..s.line_len {
        let b = s.line[i];
        push_pend(s, b);
    }
    if newline {
        push_pend(s, b'\n');
    }
    s.line_len = 0;
}

/// Feed one raw byte through the discipline.
fn feed(mut b: u8) {
    ensure_defaults();
    let s = st();
    let lflag = s.lflag;
    if lflag & ICANON == 0 {
        // Raw: deliver immediately (VMIN=1), echo when asked.
        let out = if s.iflag & ICRNL != 0 && b == b'\r' { b'\n' } else { b };
        push_pend(s, out);
        echo(&[b]);
        return;
    }
    // Canonical input editing.
    let erase = s.cc[VERASE];
    let eof = s.cc[VEOF];
    if lflag & ISIG != 0 && b == s.cc[VINTR] {
        echo(b"^C\r\n");
        s.line_len = 0;
        s.pend_len = 0;
        crate::process::signal_console(fantuan_abi::SIGINT);
        return;
    }
    if lflag & ISIG != 0 && b == s.cc[VQUIT] {
        echo(b"^\\\r\n");
        s.line_len = 0;
        s.pend_len = 0;
        crate::process::signal_console(fantuan_abi::SIGQUIT);
        return;
    }
    if lflag & ISIG != 0 && b == 0x1A {
        // ^Z: no stop support yet; discard the line (documented P2 limit).
        echo(b"^Z\r\n");
        s.line_len = 0;
        return;
    }
    if b == erase || b == 0x08 {
        if s.line_len > 0 {
            s.line_len -= 1;
            if lflag & ECHOE != 0 {
                echo(b"\x08 \x08");
            }
        }
        return;
    }
    if b == s.cc[VKILL] {
        while s.line_len > 0 {
            s.line_len -= 1;
            if lflag & ECHOE != 0 {
                echo(b"\x08 \x08");
            }
        }
        return;
    }
    if b == eof {
        if s.line_len > 0 {
            complete_line(s, false);
        } else {
            s.eof_once = true;
        }
        return;
    }
    if b == b'\r' {
        if s.iflag & ICRNL != 0 {
            b = b'\n';
        }
    }
    if b == b'\n' {
        if lflag & ECHONL == 0 {
            echo(b"\r\n");
        }
        complete_line(s, true);
        return;
    }
    if lflag & IXON != 0 && (b == s.cc[7] || b == s.cc[8]) {
        return; // software flow control ignored (no output queue)
    }
    if s.line_len < LINE_MAX {
        s.line[s.line_len] = b;
        s.line_len += 1;
        if b >= 0x20 || b == b'\t' {
            echo(&[b]);
        }
    }
}

/// Blocking console read: returns one canonical line (or EOF on ^D at an
/// empty line, or bytes as typed in raw mode).
pub fn read(out: &mut [u8]) -> Result<usize, u64> {
    ensure_defaults();
    if out.is_empty() {
        return Ok(0);
    }
    loop {
        {
            let _g = lock();
            let s = st();
            if s.pend_len > 0 {
                let take = s.pend_len.min(out.len());
                out[..take].copy_from_slice(&s.pend[..take]);
                let rest = s.pend_len - take;
                for i in 0..rest {
                    s.pend[i] = s.pend[take + i];
                }
                s.pend_len = rest;
                return Ok(take);
            }
            if s.eof_once {
                s.eof_once = false;
                return Ok(0);
            }
        }
        match crate::input::poll_byte() {
            Some(b) => feed(b),
            None => {
                if crate::process::pending_handled(task::current_slot()) {
                    return Err(fantuan_abi::SYS_ERR_INTR);
                }
                task::sleep_ms(1);
            }
        }
    }
}

/// Non-blocking: any completed data waiting?
pub fn readable() -> bool {
    let _g = lock();
    let s = st();
    s.pend_len > 0 || s.eof_once
}

pub fn _notty() -> u64 {
    SYS_ERR_NOTTY
}
