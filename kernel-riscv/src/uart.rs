//! NS16550 MMIO UART driver (QEMU virt, 0x10000000) and the park loop. The
//! formatting helpers live in `kernel_core::uconsole` and are re-exported
//! here, so `uart::puts` and `uart::log_bytes` keep resolving for the callers
//! that already name them.

use core::arch::asm;

/// QEMU virt NS16550 UART (DESIGN §14.1, spike-verified).
pub const UART_BASE: usize = 0x1000_0000;

pub use kernel_core::uconsole::{log_bytes, put_bytes, put_dec, put_hex, puts, LOGO};

/// Point the shared console at this driver. **First statement of the entry
/// function**: `puts` and the log sink go through the shared hook, so until
/// this runs they write nothing at all.
pub fn install() {
    kernel_core::uconsole::set_putc(uart_putc);
}

pub fn uart_putc(c: u8) {
    // Follow the paging access base: identity while bare, the alias once the
    // high half is online. User roots share the alias but not the identity.
    let base = UART_BASE as u64 + crate::paging::access_base();
    unsafe { core::ptr::write_volatile(base as *mut u8, c) }
}

/// Non-blocking byte read (LSR bit 0 = data ready, RBR at offset 0).
pub fn getc() -> Option<u8> {
    let base = UART_BASE as u64 + crate::paging::access_base();
    let ready = unsafe { core::ptr::read_volatile((base + 5) as *const u8) } & 1;
    if ready == 0 {
        return None;
    }
    Some(unsafe { core::ptr::read_volatile(base as *const u8) })
}

/// Wait for the next interrupt forever (idle loop). Stays per-arch: `wfi` is
/// an instruction, and this module is where the port's instructions live.
pub fn park() -> ! {
    loop {
        unsafe { asm!("wfi") }
    }
}
