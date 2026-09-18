//! i686 CPU state helpers: port I/O, interrupt-state save/restore and halt.
//! Port I/O lives here so serial and the PIC/PIT share one primitive.

use core::arch::asm;

pub fn outb(port: u16, v: u8) {
    unsafe { asm!("out dx, al", in("dx") port, in("al") v, options(nomem, nostack, preserves_flags)) };
}

pub fn inb(port: u16) -> u8 {
    let v: u8;
    unsafe { asm!("in al, dx", out("al") v, in("dx") port, options(nomem, nostack, preserves_flags)) };
    v
}

/// EFI-style save/restore for the shared allocator lock: returns EFLAGS and
/// disables interrupts; restore re-enables only when IF was set.
pub fn irq_save() -> u64 {
    let flags: u32;
    unsafe { asm!("pushfd", "pop {}", out(reg) flags, options(nomem)) };
    unsafe { asm!("cli", options(nomem, nostack)) };
    flags as u64
}

pub fn irq_restore(flags: u64) {
    if flags & 0x200 != 0 {
        unsafe { asm!("sti", options(nomem, nostack)) };
    }
}

pub fn sti() {
    unsafe { asm!("sti", options(nomem, nostack)) };
}

pub fn halt() -> ! {
    loop {
        unsafe { asm!("hlt", options(nomem, nostack)) }
    }
}
