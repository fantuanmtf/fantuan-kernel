//! Authoritative GDT + TSS.
//!
//! entry.S loads a stage-0 GDT for hygiene; this module installs the real one
//! (same code/data segments, plus the TSS descriptor) during kmain. The TSS
//! provides the IST1 stack for #DF (DESIGN.md §2).

use core::arch::asm;

use crate::consts::{GDT_CODE64, GDT_DATA64, GDT_USER_CODE, GDT_USER_DATA, IST_DOUBLE_FAULT, TSS_SEL};

const TSS_LIMIT: u64 = 0x67; // size_of::<Tss>() - 1
/// I/O permission bitmap offset. Pointing it past the TSS limit means "no
/// bitmap", so every IN/OUT from CPL 3 raises #GP — user tasks must never
/// touch the PIT/PIC/serial ports directly (the syscall ABI is the only
/// kernel door).
const TSS_IOMAP_BASE: u16 = TSS_LIMIT as u16 + 1;
// null, code64, data64, tss_lo, tss_hi, user_data, user_code
const GDT_ENTRIES: usize = 7;

#[repr(C, packed)]
pub struct Tss {
    _reserved0: u32,
    pub rsp0: u64,
    pub rsp1: u64,
    pub rsp2: u64,
    _reserved1: u64,
    pub ist1: u64,
    pub ist2: u64,
    pub ist3: u64,
    pub ist4: u64,
    pub ist5: u64,
    pub ist6: u64,
    pub ist7: u64,
    _reserved2: u64,
    _reserved3: u16,
    pub iomap_base: u16,
}

#[allow(dead_code)] // the buffer is used via addr_of! (stack top)
#[repr(align(16))]
struct Stack([u8; 16 * 1024]);

static mut TSS: Tss = Tss {
    _reserved0: 0,
    rsp0: 0,
    rsp1: 0,
    rsp2: 0,
    _reserved1: 0,
    ist1: 0,
    ist2: 0,
    ist3: 0,
    ist4: 0,
    ist5: 0,
    ist6: 0,
    ist7: 0,
    _reserved2: 0,
    _reserved3: 0,
    iomap_base: TSS_IOMAP_BASE,
};
static mut DF_STACK: Stack = Stack([0; 16 * 1024]);
static mut GDT: [u64; GDT_ENTRIES] = [0; GDT_ENTRIES];

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

/// System descriptor for the 64-bit TSS (type 0x89, base in bits 16..39/56..63).
fn tss_low(base: u64) -> u64 {
    (TSS_LIMIT & 0xFFFF)
        | ((base & 0xFF_FFFF) << 16)
        | (0x89u64 << 40)
        | (((TSS_LIMIT >> 16) & 0xF) << 48)
        | (((base >> 24) & 0xFF) << 56)
}

pub fn init() {
    let tss_base = core::ptr::addr_of!(TSS) as u64;
    let df_top = (core::ptr::addr_of!(DF_STACK) as *const u8 as u64) + 16 * 1024;

    unsafe {
        TSS.ist1 = df_top;
        GDT = [
            0,
            GDT_CODE64,
            GDT_DATA64,
            tss_low(tss_base),
            tss_base >> 32,
            GDT_USER_DATA,
            GDT_USER_CODE,
        ];
        let gdtr = DescriptorTablePointer {
            limit: (GDT_ENTRIES * 8 - 1) as u16,
            base: core::ptr::addr_of!(GDT) as u64,
        };
        asm!("lgdt [{}]", in(reg) &gdtr, options(nostack));
        asm!("ltr ax", in("ax") TSS_SEL, options(nostack));
    }

    let _ = IST_DOUBLE_FAULT; // IST1 is wired in idt.rs; keep the const referenced here too
}

/// Update the ring-0 stack for interrupts coming from ring 3 (M4). The
/// scheduler sets this to the current task's kernel stack top before iretq.
pub fn set_rsp0(top: u64) {
    unsafe {
        TSS.rsp0 = top;
    }
}
