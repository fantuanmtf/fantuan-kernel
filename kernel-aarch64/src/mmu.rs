//! aarch64 stage-1 MMU (M11 R9a): 4K granule, 48-bit VA, TTBR0 identity +
//! TTBR1 direct map at PHYS_OFFSET (0xFFFF_0000_0000_0000). RAM uses 2 MiB
//! L2 blocks; the UART/GIC windows are 2 MiB device blocks. Tables are built
//! while the MMU is off (physical access); `use_alias()` flips table access
//! to the direct map once it is live, so a future user-mapping path can keep
//! building tables.
//!
//! MAIR attr0 = normal write-back, attr1 = device nGnRE. SCTLR sets only
//! M/C/I plus the RES1 bits QEMU expects; WXN stays 0 because the kernel
//! image is mapped RWX for simplicity (R9a has no userland/W^X split yet).

use core::arch::asm;

use kernel_core::frame;

/// arm64 linear-map base (abi cfg arm); the top 16 bits select TTBR1.
pub const PHYS_OFFSET: u64 = fantuan_abi::PHYS_OFFSET;

pub const fn phys_to_virt(p: u64) -> u64 {
    PHYS_OFFSET + p
}

const BLOCK_2M: u64 = 2 * 1024 * 1024;

// Descriptor bits (Arm ARM D8.3, stage 1).
const VALID: u64 = 1 << 0;
const TABLE: u64 = 1 << 1;
const ATTR_NORMAL: u64 = 0 << 2; // MAIR attr0
const ATTR_DEVICE: u64 = 1 << 2; // MAIR attr1
const AP_RW_EL1: u64 = 0 << 6;
const SH_INNER: u64 = 3 << 8;
const AF: u64 = 1 << 10;
const PXN: u64 = 1 << 53;
const UXN: u64 = 1 << 54;

/// TCR_EL1: T0SZ/T1SZ=16 (48-bit VA), WB cacheable walks, inner shareable,
/// 4K granule both halves, IPS=2 (40-bit PA).
const TCR_EL1: u64 =
    16 | (16 << 16) | (1 << 8) | (1 << 10) | (3 << 12) | (1 << 24) | (1 << 26) | (3 << 28) | (2 << 30) | (2 << 32);
/// M/C/I plus the RES1 bits (EOS/TSCXT/EIS/SPAN/nTLSMD/LSMAOE).
const SCTLR_EL1: u64 = (1 << 0) | (1 << 2) | (1 << 12) | (1 << 11) | (1 << 20) | (1 << 22) | (1 << 23) | (1 << 28) | (1 << 29);

/// Table access base: 0 while bare, PHYS_OFFSET after `use_alias()`.
static mut ACCESS: u64 = 0;
static mut ROOT_LOW: u64 = 0;
static mut ROOT_HIGH: u64 = 0;

/// TTBR0 (identity) root; kernel tasks keep this as their vm_root.
pub fn kernel_root() -> u64 {
    unsafe { ROOT_LOW }
}

/// Switch table accesses to the direct map (call once the MMU is on).
pub fn use_alias() {
    unsafe { ACCESS = PHYS_OFFSET };
}

fn at(phys: u64) -> *mut u64 {
    (phys + unsafe { ACCESS }) as *mut u64
}

fn zero_frame(phys: u64) {
    let p = at(phys);
    for i in 0..512 {
        unsafe { core::ptr::write_volatile(p.add(i), 0) };
    }
}

/// Get (allocating when empty) the next-level table for INDEX.
fn table(parent_phys: u64, index: usize) -> u64 {
    let e = unsafe { core::ptr::read_volatile(at(parent_phys).add(index)) };
    if e & VALID != 0 {
        return e & !0xFFF;
    }
    let f = frame::get().alloc().expect("no frame for an aarch64 table");
    zero_frame(f);
    unsafe { core::ptr::write_volatile(at(parent_phys).add(index), f | VALID | TABLE) };
    f
}

fn map_2m(root: u64, va: u64, pa: u64, flags: u64) {
    let l0 = ((va >> 39) & 0x1FF) as usize;
    let l1t = table(root, l0);
    let l1 = ((va >> 30) & 0x1FF) as usize;
    let l2t = table(l1t, l1);
    let l2 = ((va >> 21) & 0x1FF) as usize;
    let desc = (pa & !(BLOCK_2M - 1)) | flags | VALID;
    unsafe { core::ptr::write_volatile(at(l2t).add(l2), desc) };
}

/// RAM flags: normal write-back, EL1 RW, executable (kernel image), EL0
/// inaccessible (UXN).
pub fn ram_flags() -> u64 {
    ATTR_NORMAL | AP_RW_EL1 | SH_INNER | AF | UXN
}

/// Device flags: nGnRE, EL1 RW, never executable at any EL.
pub fn mmio_flags() -> u64 {
    ATTR_DEVICE | AP_RW_EL1 | AF | PXN | UXN
}

/// Allocate the two roots; call before the `map_*` calls.
pub fn init() {
    let low = frame::get().alloc().expect("no frame for the TTBR0 root");
    zero_frame(low);
    let high = frame::get().alloc().expect("no frame for the TTBR1 root");
    zero_frame(high);
    unsafe {
        ROOT_LOW = low;
        ROOT_HIGH = high;
    }
}

/// Map one 2 MiB-aligned RAM chunk identity plus at the direct map. BASE is
/// physical; the kernel links at its physical address, so identity is the
/// boot-time view and PHYS_OFFSET is the frame-allocator view.
pub fn map_ram(base: u64) {
    let pa = base & !(BLOCK_2M - 1);
    map_2m(unsafe { ROOT_LOW }, pa, pa, ram_flags());
    map_2m(unsafe { ROOT_HIGH }, phys_to_virt(pa), pa, ram_flags());
}

/// Map one 2 MiB-aligned device chunk identity (UART/GIC).
pub fn map_mmio(base: u64) {
    let pa = base & !(BLOCK_2M - 1);
    map_2m(unsafe { ROOT_LOW }, pa, pa, mmio_flags());
}

/// Write MAIR/TCR/TTBR/SCTLR and flush the TLBs.
pub fn enable() {
    let mair: u64 = 0xFF | (0x04 << 8);
    unsafe {
        asm!("msr mair_el1, {}", in(reg) mair, options(nostack));
        asm!("msr tcr_el1, {}", in(reg) TCR_EL1, options(nostack));
        asm!("msr ttbr0_el1, {}", in(reg) ROOT_LOW, options(nostack));
        asm!("msr ttbr1_el1, {}", in(reg) ROOT_HIGH, options(nostack));
        asm!("dsb ishst", options(nostack));
        asm!("tlbi vmalle1", options(nostack));
        asm!("dsb ish", options(nostack));
        asm!("isb", options(nostack));
        asm!("msr sctlr_el1, {}", in(reg) SCTLR_EL1, options(nostack));
        asm!("isb", options(nostack));
    }
}
