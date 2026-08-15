//! Post-exit serial output — the debug channel across the handoff boundary.
//! Port I/O is plain hardware, NOT a boot service: it survives the exit.
//!
//! Deliberately NOT a shared crate yet: the kernel has its own 16550 driver
//! with fmt::Write support. Revisit when the M4 C-driver layer defines the
//! driver model.

pub fn ser_outb(port: u16, v: u8) {
    unsafe {
        core::arch::asm!("out dx, al", in("dx") port, in("al") v, options(nomem, nostack, preserves_flags));
    }
}

pub fn ser_inb(port: u16) -> u8 {
    let v: u8;
    unsafe {
        core::arch::asm!("in al, dx", out("al") v, in("dx") port, options(nomem, nostack, preserves_flags));
    }
    v
}

pub fn init() {
    ser_outb(0x3F8 + 1, 0x00);
    ser_outb(0x3F8 + 3, 0x80);
    ser_outb(0x3F8 + 0, 0x01);
    ser_outb(0x3F8 + 1, 0x00);
    ser_outb(0x3F8 + 3, 0x03);
    ser_outb(0x3F8 + 2, 0xC7);
    ser_outb(0x3F8 + 4, 0x0B);
}

pub fn puts(s: &str) {
    for &b in s.as_bytes() {
        while ser_inb(0x3F8 + 5) & 0x20 == 0 {}
        ser_outb(0x3F8, b);
    }
}

pub fn hex(v: u64) {
    const HEX: &[u8] = b"0123456789ABCDEF";
    puts("0x");
    let mut started = false;
    for shift in (0..16).rev() {
        let nib = ((v >> (shift * 4)) & 0xF) as usize;
        if nib != 0 || started || shift == 0 {
            while ser_inb(0x3F8 + 5) & 0x20 == 0 {}
            ser_outb(0x3F8, HEX[nib]);
            started = true;
        }
    }
}

pub fn line(s: &str) {
    puts(s);
    puts("\r\n");
}
