//! fantuan-abi — the versioned ABI between the bootloader and the kernel.
//!
//! See docs/DESIGN.md §4.1. Append-only: fields may only ever be added, never
//! removed or reordered. Any change here breaks the boot chain on both sides —
//! this crate exists so the struct is defined in exactly one place.

#![no_std]

pub const BOOT_MAGIC: u32 = 0x4654_4E46; // "FTNF"
pub const BOOT_VERSION: u32 = 1;

/// EFI memory type for free, usable RAM.
pub const MEMORY_TYPE_CONVENTIONAL: u32 = 7;

/// Must match the EFI_MEMORY_DESCRIPTOR layout the firmware returns.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryDescriptor {
    pub type_: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemMap {
    pub ptr: *const MemoryDescriptor,
    pub count: usize,
    pub desc_size: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FrameBuffer {
    pub base: u64,
    pub size: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32, // pixels per scanline
    pub format: u32, // 0=RGB8 1=BGR8 2=bitmask
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BootInfo {
    pub magic: u32,
    pub version: u32,
    pub memmap: MemMap,
    pub fb: FrameBuffer,
    pub rsdp: u64,
    pub kernel_base: u64,
    pub stack_top: u64,
    pub caps: u64,
}
