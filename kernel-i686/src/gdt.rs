//! i686 GDT + TSS (M10-4b3): flat ring-0/ring-3 segments plus a TSS so the
//! CPU can find the kernel stack when a ring-3 task traps into ring 0
//! (int 0x80 or an interrupt while user code runs).

use core::arch::asm;
use core::mem::size_of;
use core::ptr;

pub const KDATA: u16 = 0x10;
pub const UCODE: u16 = 0x18;
pub const UDATA: u16 = 0x20;
/// Ring-3 selectors: for non-conforming code segments the iret CS RPL must
/// equal the descriptor DPL, so the frames use the RPL-3 forms.
pub const UCODE_SEL: u16 = UCODE | 3; // 0x1B
pub const UDATA_SEL: u16 = UDATA | 3; // 0x23
pub const TSS_SEL: u16 = 0x28;

#[repr(C, packed)]
struct Tss {
    prev: u32,
    esp0: u32,
    ss0: u32,
    esp1: u32,
    ss1: u32,
    esp2: u32,
    ss2: u32,
    cr3: u32,
    eip: u32,
    eflags: u32,
    eax: u32,
    ecx: u32,
    edx: u32,
    ebx: u32,
    esp: u32,
    ebp: u32,
    esi: u32,
    edi: u32,
    es: u32,
    cs: u32,
    ss: u32,
    ds: u32,
    fs: u32,
    gs: u32,
    ldt: u32,
    trap: u16,
    iomap: u16,
}

static mut TSS: Tss = Tss {
    prev: 0,
    esp0: 0,
    ss0: 0,
    esp1: 0,
    ss1: 0,
    esp2: 0,
    ss2: 0,
    cr3: 0,
    eip: 0,
    eflags: 0,
    eax: 0,
    ecx: 0,
    edx: 0,
    ebx: 0,
    esp: 0,
    ebp: 0,
    esi: 0,
    edi: 0,
    es: 0,
    cs: 0,
    ss: 0,
    ds: 0,
    fs: 0,
    gs: 0,
    ldt: 0,
    trap: 0,
    iomap: 0,
};

const GDT_LEN: usize = 6;
static mut GDT: [u64; GDT_LEN] = [0; GDT_LEN];

#[repr(C, packed)]
pub struct DescriptorPtr {
    limit: u16,
    base: u32,
}

extern "C" {
    fn gdt_flush(p: *const DescriptorPtr);
}

/// Set esp0: the kernel stack the CPU switches to on ring-3 traps.
pub fn set_kernel_stack(top: u64) {
    unsafe { ptr::addr_of_mut!(TSS.esp0).write_volatile(top as u32) };
}

pub fn init() {
    unsafe {
        GDT[1] = 0x00CF_9A00_0000_FFFF; // ring0 code, DPL0, 4 GiB flat
        GDT[2] = 0x00CF_9200_0000_FFFF; // ring0 data
        GDT[3] = 0x00CF_FA00_0000_FFFF; // ring3 code
        GDT[4] = 0x00CF_F200_0000_FFFF; // ring3 data

        let base = ptr::addr_of!(TSS) as u32;
        let limit = (size_of::<Tss>() - 1) as u32;
        GDT[5] = (limit as u64 & 0xFFFF)
            | (((base & 0xFFFF) as u64) << 16)
            | ((((base >> 16) & 0xFF) as u64) << 32)
            | (0x89u64 << 40) // present, DPL0, 32-bit available TSS
            | ((((limit >> 16) & 0xF) as u64) << 48)
            | ((((base >> 24) & 0xFF) as u64) << 56);

        TSS.ss0 = KDATA as u32;
        TSS.iomap = size_of::<Tss>() as u16; // no I/O bitmap
    }

    let gdtp = DescriptorPtr {
        limit: (GDT_LEN * 8 - 1) as u16,
        base: ptr::addr_of!(GDT) as u32,
    };
    unsafe {
        gdt_flush(&gdtp);
        asm!("ltr ax", in("ax") TSS_SEL, options(nostack, nomem));
    }
}
