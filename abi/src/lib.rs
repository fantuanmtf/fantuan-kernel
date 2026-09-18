//! fantuan-abi — the versioned ABI between the bootloader and the kernel.
//!
//! See docs/DESIGN.md §4.1. Append-only: fields may only ever be added, never
//! removed or reordered. Any change here breaks the boot chain on both sides —
//! this crate exists so the struct is defined in exactly one place.

#![no_std]

pub const BOOT_MAGIC: u32 = 0x4654_4E46; // "FTNF"
/// v2 (M9.2): append-only arch/hartid/dtb fields for the RISC-V boot path.
pub const BOOT_VERSION: u32 = 2;

/// Physical-to-virtual mapping convention (DESIGN.md §4.5): every physical
/// address p is reachable at PHYS_OFFSET + p. Shared by the bootloader (which
/// builds the initial page tables) and the kernel (which is linked there).
#[cfg(target_arch = "x86_64")]
pub const PHYS_OFFSET: u64 = 0xFFFF_8000_0000_0000; // -2 GiB, Linux-style
/// Sv39's canonical high half (bits 63:39 sign-extend bit 38).
#[cfg(target_arch = "riscv64")]
pub const PHYS_OFFSET: u64 = 0xFFFF_FFC0_0000_0000;
/// i686 (M10): 3G/1G split; the direct map covers the first 1 GiB of RAM
/// (see docs/M10_BOOT_32BIT.md 7.5).
#[cfg(target_arch = "x86")]
pub const PHYS_OFFSET: u64 = 0xC000_0000;

/// Pages of initial kernel stack allocated by the bootloader.
pub const BOOT_STACK_PAGES: u64 = 16;

// --- Syscall ABI v1 (shared by kernel and userland) ------------------------

pub const SYS_VERSION: u64 = 0;
pub const SYS_EXIT: u64 = 1;
pub const SYS_SLEEP_MS: u64 = 2;
pub const SYS_WRITE: u64 = 3; // (buf, len): kernel debug channel (serial)
pub const SYS_GET_TID: u64 = 4;
pub const SYS_YIELD: u64 = 5;

pub const SYS_OK: u64 = 0;
pub const SYS_ERR_NOSYS: u64 = u64::MAX; // -1
pub const SYS_ERR_INVAL: u64 = u64::MAX - 1; // -2

// --- User-mode layout (M4) -------------------------------------------------

/// User data segment selector (GDT index 5, RPL 3).
pub const USER_DS_SEL: u16 = 0x2B;
/// User code segment selector (GDT index 6, RPL 3).
pub const USER_CS_SEL: u16 = 0x33;
/// User stack top (4 frames mapped at this base).
pub const USER_STACK_TOP: u64 = 0x300000 + 4 * 4096;

/// EFI memory type for free, usable RAM.
pub const MEMORY_TYPE_CONVENTIONAL: u32 = 7;

/// Must match the EFI_MEMORY_DESCRIPTOR layout the firmware returns. The
/// explicit pad keeps the offsets identical on 32-bit targets, where a u64
/// field would otherwise align to 4 bytes (M10 i686).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryDescriptor {
    pub type_: u32,
    pub _pad0: u32,
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

#[cfg(target_pointer_width = "64")]
#[repr(C)]
pub struct BootInfo {
    pub magic: u32,
    pub version: u32,
    pub memmap: MemMap,
    pub fb: FrameBuffer,
    pub rsdp: u64,
    pub kernel_base: u64,
    pub stack_top: u64,
    pub caps: u64,
    // M2 additions (append-only; older fields never move):
    /// Physical address of the bootloader-built initial PML4.
    pub boot_pml4: u64,
    /// Number of 4K pages the bootloader's page tables occupy (the frame
    /// allocator must keep them used until the kernel switches away).
    pub boot_tables_pages: u64,
    // M7.5 addition:
    /// Physical address of the UEFI Runtime Services table. Runtime services
    /// survive ExitBootServices; the kernel calls them with physical
    /// addresses (no SetVirtualAddressMap; the 4 GiB identity map covers it).
    pub runtime_services: u64,
    // M5.5 addition:
    /// Physical address of the SMBIOS entry point structure found in the UEFI
    /// configuration table (SMBIOS3 preferred), or 0 when the firmware
    /// publishes none. The kernel parses the structure table from here.
    pub smbios_table: u64,
    // M9.2 additions (append-only):
    /// Boot path discriminator: 1 = x86_64 UEFI, 2 = riscv64 OpenSBI,
    /// 3 = BIOS (x86_64 or i686).
    pub arch: u32,
    /// RISC-V: hart id from the OpenSBI handoff (0 on x86_64).
    pub hartid: u64,
    /// RISC-V: physical DTB address from the OpenSBI handoff (0 on x86_64).
    pub dtb: u64,
}

/// 32-bit BootInfo (i686 BIOS): the same field order with pointer-width
/// `usize` fields and 4-byte u64 alignment. Stage2 writes these offsets for
/// the i686 build (docs/M10_BOOT_32BIT.md 7.6).
#[cfg(target_pointer_width = "32")]
#[repr(C)]
pub struct BootInfo {
    pub magic: u32,
    pub version: u32,
    pub memmap: MemMap,
    pub fb: FrameBuffer,
    pub rsdp: u64,
    pub kernel_base: u64,
    pub stack_top: u64,
    pub caps: u64,
    pub boot_pml4: u64,
    pub boot_tables_pages: u64,
    pub runtime_services: u64,
    pub smbios_table: u64,
    pub arch: u32,
    pub hartid: u64,
    pub dtb: u64,
}
