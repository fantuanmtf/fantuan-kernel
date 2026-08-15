//! Arch constants — the single source of truth for both Rust and assembly.
//!
//! kernel/build.rs parses this file into asm_defs.inc (DESIGN.md §13.3);
//! assembly files include the generated .inc. Raw hex in assembly is forbidden.

// GDT selectors (stage-0 GDT in boot/entry.S and the authoritative GDT in
// gdt.rs share these values).
pub const KERNEL_CS: u16 = 0x08;
#[allow(dead_code)] // referenced from boot/entry.S via the generated asm_defs.inc
pub const KERNEL_DS: u16 = 0x10;
/// Index 3: a 64-bit TSS descriptor spans TWO GDT slots (3 and 4), so the
/// selector points at the first half.
pub const TSS_SEL: u16 = 0x18;

pub const GDT_CODE64: u64 = 0x00AF_9A00_0000_FFFF;
pub const GDT_DATA64: u64 = 0x00AF_9200_0000_FFFF;

pub const IDT_ENTRIES: usize = 256;
/// Present, DPL0, 64-bit interrupt gate.
pub const IDT_FLAGS_INTERRUPT: u8 = 0x8E;
pub const IST_DOUBLE_FAULT: u8 = 1;

pub const PIC1_CMD: u16 = 0x20;
pub const PIC1_DATA: u16 = 0x21;
pub const PIC2_CMD: u16 = 0xA0;
pub const PIC2_DATA: u16 = 0xA1;
pub const PIC1_OFFSET: u8 = 0x20;
pub const PIC2_OFFSET: u8 = 0x28;
pub const IRQ_TIMER: u8 = 0x20; // PIC1_OFFSET + 0

pub const PIT_FREQ_HZ: u64 = 1_193_182;
