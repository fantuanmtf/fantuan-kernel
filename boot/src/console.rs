//! ConOut text helpers (ASCII only; UTF-16 on the wire, DESIGN.md §3).

use crate::uefi::protocol::SimpleTextOutput;

pub fn write_ascii(buf: &mut [u16], off: &mut usize, s: &str) {
    for b in s.bytes() {
        if *off + 2 < buf.len() {
            buf[*off] = b as u16;
            *off += 1;
        }
    }
}

pub fn write_hex64(buf: &mut [u16], off: &mut usize, v: u64) {
    const HEX: &[u8] = b"0123456789ABCDEF";
    write_ascii(buf, off, "0x");
    let mut started = false;
    for shift in (0..16).rev() {
        let nib = ((v >> (shift * 4)) & 0xF) as usize;
        if nib != 0 || started || shift == 0 {
            buf[*off] = HEX[nib] as u16;
            *off += 1;
            started = true;
        }
    }
}

pub fn write_dec(buf: &mut [u16], off: &mut usize, mut v: u64) {
    let mut tmp = [0u8; 20];
    let mut n = 0;
    if v == 0 {
        tmp[0] = b'0';
        n = 1;
    }
    while v > 0 {
        tmp[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
    }
    while n > 0 {
        buf[*off] = tmp[n - 1] as u16;
        *off += 1;
        n -= 1;
    }
}

pub fn output_line(con: *mut SimpleTextOutput, buf: &mut [u16], off: usize) {
    buf[off] = b'\n' as u16;
    buf[off + 1] = b'\r' as u16;
    buf[off + 2] = 0;
    unsafe {
        ((*con).output_string)(con, buf.as_mut_ptr());
    }
}

pub fn println(con: *mut SimpleTextOutput, s: &str) {
    let mut buf = [0u16; 256];
    let mut off = 0;
    write_ascii(&mut buf, &mut off, s);
    output_line(con, &mut buf, off);
}

pub fn println_hex(con: *mut SimpleTextOutput, label: &str, v: u64) {
    let mut buf = [0u16; 256];
    let mut off = 0;
    write_ascii(&mut buf, &mut off, label);
    write_hex64(&mut buf, &mut off, v);
    output_line(con, &mut buf, off);
}

/// Boot splash lines (ASCII logo + release signature).
pub const LOGO: [&str; 6] = [
    "  __                _",
    " / _| __ _ _ __  | |_ _   _  __ _ _ __",
    "| |_ / _` | '  \\| __| | | |/ _` | '  \\",
    "|  _| (_| | | | | |_| |_| | (_| | | | |",
    "|_|  \\__,_|_| |_|\\__|\\__,_|\\__,_|_| |_|",
    "  fantuan v0.0.2 - fantuan-is-mtf",
];
