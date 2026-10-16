//! PL011 UART driver (QEMU virt, 0x0900_0000) and the park loop. The identity
//! map keeps this physical address valid before and after the MMU comes up,
//! so no access-base switch is needed here. The formatting helpers live in
//! `kernel_core::uconsole` and are re-exported, so `uart::puts` and
//! `uart::log_bytes` keep resolving for the callers that already name them.

use core::arch::asm;

/// QEMU virt PL011 (hardcoded; the FDT scan only sanity-checks it).
pub const UART_BASE: usize = 0x0900_0000;

const DR: usize = 0x00;
const FR: usize = 0x18;
const IBRD: usize = 0x24;
const FBRD: usize = 0x28;
const LCR_H: usize = 0x2C;
const CR: usize = 0x30;

fn rd(off: usize) -> u32 {
    unsafe { core::ptr::read_volatile((UART_BASE + off) as *const u32) }
}

fn wr(off: usize, v: u32) {
    unsafe { core::ptr::write_volatile((UART_BASE + off) as *mut u32, v) }
}

/// 8N1, FIFOs on, TX/RX enabled. QEMU ignores the divider; the values are
/// the real 115200 @ 24 MHz ones so the code stays correct off QEMU.
pub fn init() {
    wr(CR, 0);
    wr(IBRD, 13);
    wr(FBRD, 1);
    wr(LCR_H, 0x70); // WLEN=8, FIFO enable
    wr(CR, 1 | (1 << 8) | (1 << 9)); // UARTEN | TXE | RXE
}

pub use kernel_core::uconsole::{log_bytes, put_bytes, put_dec, put_hex, puts, LOGO};

/// Point the shared console at this driver. **Call it at the top of the
/// entry**, before `init` and before anything prints: `puts` and the log sink
/// go through the shared hook, so until this runs they write nothing at all.
pub fn install() {
    kernel_core::uconsole::set_putc(uart_putc);
}

pub fn uart_putc(c: u8) {
    while rd(FR) & (1 << 5) != 0 {} // TXFF
    wr(DR, c as u32);
}

/// Non-blocking byte read (FR bit 4 = RXFE).
pub fn getc() -> Option<u8> {
    if rd(FR) & (1 << 4) != 0 {
        return None;
    }
    Some(rd(DR) as u8)
}

/// Wait for the next interrupt forever (idle loop). Stays per-arch: `wfi` is
/// an instruction, and this module is where the port's instructions live.
pub fn park() -> ! {
    loop {
        unsafe { asm!("wfi") }
    }
}
