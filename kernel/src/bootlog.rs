//! Boot-time log and halt helpers shared by kmain and the panic handler.
//! Split out of main.rs to keep every file inside the size rule.

use core::fmt::Write;

use fantuan_abi::BootInfo;

use crate::serial::{Serial, COM1};

/// Serial output channel 1 (COM1), used before and after the shell starts.
pub fn serial() -> Serial {
    Serial::new(COM1)
}

pub fn print_memmap_summary(s: &mut Serial, bi: &BootInfo) {
    let n = bi.memmap.count;
    let mut conv_pages: u64 = 0;
    let mut other_pages: u64 = 0;
    unsafe {
        let mut ptr = bi.memmap.ptr;
        for _ in 0..n {
            let d = &*ptr;
            if d.type_ == fantuan_abi::MEMORY_TYPE_CONVENTIONAL {
                conv_pages += d.number_of_pages;
            } else {
                other_pages += d.number_of_pages;
            }
            ptr = (ptr as *const u8).add(bi.memmap.desc_size) as *const _;
        }
    }
    let _ = writeln!(
        s,
        "memory map: {} descriptors, conventional {} MiB, other {} MiB",
        n,
        conv_pages * 4 / 1024,
        other_pages * 4 / 1024
    );
}

/// Fatal stop: park the CPU with interrupts disabled. No return by design.
pub fn halt_forever() -> ! {
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}
