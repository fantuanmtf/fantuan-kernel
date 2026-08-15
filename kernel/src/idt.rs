//! IDT: 256 entries built from the isr_* stubs in kernel/src/asm/interrupts.S.

use core::arch::asm;

use crate::consts::{IDT_ENTRIES, IDT_FLAGS_INTERRUPT, IST_DOUBLE_FAULT, KERNEL_CS};
use crate::exceptions::EXC_DOUBLE_FAULT;

include!(concat!(env!("OUT_DIR"), "/isr_table.rs"));

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IdtEntry {
    offset_lo: u16,
    pub selector: u16,
    pub ist: u8,
    pub flags: u8,
    offset_mid: u16,
    offset_hi: u32,
    pub reserved: u32,
}

impl IdtEntry {
    const fn missing() -> Self {
        Self {
            offset_lo: 0,
            selector: 0,
            ist: 0,
            flags: 0,
            offset_mid: 0,
            offset_hi: 0,
            reserved: 0,
        }
    }

    fn new(addr: u64, ist: u8) -> Self {
        Self::with_dpl(addr, ist, 0)
    }

    /// Gate callable from the given privilege level (M4: the syscall gate is
    /// DPL 3 so userland can issue INT 0x60).
    fn with_dpl(addr: u64, ist: u8, dpl: u8) -> Self {
        Self {
            offset_lo: addr as u16,
            selector: KERNEL_CS,
            ist,
            flags: IDT_FLAGS_INTERRUPT | (dpl << 5),
            offset_mid: (addr >> 16) as u16,
            offset_hi: (addr >> 32) as u32,
            reserved: 0,
        }
    }
}

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

static mut IDT: [IdtEntry; IDT_ENTRIES] = [IdtEntry::missing(); IDT_ENTRIES];

pub fn init() {
    let addrs = isr_addresses();
    unsafe {
        for i in 0..IDT_ENTRIES {
            IDT[i] = IdtEntry::new(addrs[i], 0);
        }
        // #DF runs on its own stack (IST1) so a corrupted kernel stack cannot
        // turn a double fault into a triple fault.
        IDT[EXC_DOUBLE_FAULT] = IdtEntry::new(addrs[EXC_DOUBLE_FAULT], IST_DOUBLE_FAULT);
        // The syscall gate is callable from ring 3 (M4).
        IDT[crate::syscall::SYSCALL_VECTOR as usize] =
            IdtEntry::with_dpl(addrs[crate::syscall::SYSCALL_VECTOR as usize], 0, 3);

        let idtr = DescriptorTablePointer {
            limit: (IDT_ENTRIES * core::mem::size_of::<IdtEntry>() - 1) as u16,
            base: core::ptr::addr_of!(IDT) as u64,
        };
        asm!("lidt [{}]", in(reg) &idtr, options(nostack));

        // M4 debug: dump the syscall gate as built
        let e = &IDT[crate::syscall::SYSCALL_VECTOR as usize];
        crate::serial::line("idt: syscall gate addr");
        crate::serial::hex(addrs[crate::syscall::SYSCALL_VECTOR as usize]);
        let stored = ((e.offset_hi as u64) << 32) | ((e.offset_mid as u64) << 16) | e.offset_lo as u64;
        crate::serial::line("idt: syscall gate stored");
        crate::serial::hex(stored);
    }
}
