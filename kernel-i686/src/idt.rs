//! i686 interrupt descriptor table (M10-4b2): 48 vectors (0..31 CPU
//! exceptions, 32..47 remapped PIC IRQs). Each stub jumps to `isr_common`,
//! which calls `isr_dispatch`, drops the vector plus the dummy/error word and
//! irets.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};

use crate::{pic, pit, serial};

extern "C" {
    static isr_table: [u32; 48];
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_lo: u16,
    selector: u16,
    zero: u8,
    flags: u8,
    offset_hi: u16,
}

#[repr(C, packed)]
struct IdtPtr {
    limit: u16,
    base: u32,
}

static mut IDT: [IdtEntry; 48] = [IdtEntry {
    offset_lo: 0,
    selector: 0,
    zero: 0,
    flags: 0,
    offset_hi: 0,
}; 48];

const EXC_NAMES: [&str; 32] = [
    "divide error", "debug", "nmi", "breakpoint", "overflow", "bound range",
    "invalid opcode", "device not available", "double fault", "coprocessor",
    "invalid tss", "segment not present", "stack fault", "general protection",
    "page fault", "reserved", "x87 fpu", "alignment check", "machine check",
    "simd", "virtualization", "control protection", "reserved", "reserved",
    "reserved", "reserved", "reserved", "hypervisor", "vmm communication",
    "security", "reserved", "reserved",
];

pub fn init() {
    let table = unsafe { &*core::ptr::addr_of!(isr_table) };
    for (i, &addr) in table.iter().enumerate() {
        unsafe {
            IDT[i] = IdtEntry {
                offset_lo: addr as u16,
                selector: 0x08, // stage2's flat 32-bit code segment
                zero: 0,
                flags: 0x8E, // present, DPL0, 32-bit interrupt gate
                offset_hi: (addr >> 16) as u16,
            };
        }
    }
    let ptr = IdtPtr {
        limit: (48 * 8 - 1) as u16,
        base: core::ptr::addr_of!(IDT) as u32,
    };
    unsafe { asm!("lidt [{}]", in(reg) &ptr, options(nostack)) };
}

static EXC_COUNT: AtomicU64 = AtomicU64::new(0);

/// Called from `isr_common` (isr_stubs.S). The stub drops the vector plus
/// the dummy/error word after this returns.
#[no_mangle]
pub extern "C" fn isr_dispatch(vector: u32, error: u32) {
    if vector >= 32 {
        let irq = vector - 32;
        if irq == 0 {
            pit::tick();
            kernel_core::task::schedule();
        }
        pic::eoi(irq as u8);
        return;
    }

    serial::puts("exc ");
    serial::put_dec(vector as u64);
    if vector < 32 {
        serial::puts(" (");
        serial::puts(EXC_NAMES[vector as usize]);
        serial::puts(")");
    }
    serial::puts(" err=");
    serial::put_hex(error as u64);
    if vector == 14 {
        let cr2: u32;
        unsafe { asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack)) };
        serial::puts(" cr2=");
        serial::put_hex(cr2 as u64);
    }
    serial::puts("\n");
    EXC_COUNT.fetch_add(1, Ordering::Relaxed);

    // Recoverable software exceptions resume; everything else halts.
    match vector {
        0 | 3 | 4 => {}
        _ => crate::cpu::halt(),
    }
}

pub fn exception_count() -> u64 {
    EXC_COUNT.load(Ordering::Relaxed)
}
