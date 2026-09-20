//! BootInfo synthesis for the shared frame allocator (mirrors the riscv
//! path): the FDT memory nodes become conventional-memory descriptors, and
//! the handoff fields carry the aarch64 discriminator and the DTB address.

use core::mem::size_of;

use fantuan_abi::{
    BootInfo, FrameBuffer, MemMap, MemoryDescriptor, BOOT_MAGIC, BOOT_VERSION,
    MEMORY_TYPE_CONVENTIONAL,
};

/// QEMU virt RAM base; the kernel links at RAM_BASE + 8 MiB (link.ld). QEMU
/// loads the raw `Image` at loader_start + 0x80000 and passes the DTB in x0.
pub const RAM_BASE: u64 = 0x4000_0000;
pub const KERNEL_BASE: u64 = 0x4008_0000;
/// Boot path discriminator value for the direct-FDT aarch64 boot.
pub const ARCH_AARCH64_FDT: u32 = 4;

const EMPTY_DESC: MemoryDescriptor = MemoryDescriptor {
    type_: 0,
    _pad0: 0,
    physical_start: 0,
    virtual_start: 0,
    number_of_pages: 0,
    attribute: 0,
};

static mut MEMMAP: [MemoryDescriptor; 16] = [EMPTY_DESC; 16];
static mut BOOT_INFO: BootInfo = BootInfo {
    magic: BOOT_MAGIC,
    version: BOOT_VERSION,
    memmap: MemMap { ptr: core::ptr::null(), count: 0, desc_size: size_of::<MemoryDescriptor>() },
    fb: FrameBuffer { base: 0, size: 0, width: 0, height: 0, stride: 0, format: 0 },
    rsdp: 0,
    kernel_base: KERNEL_BASE,
    stack_top: 0,
    caps: 0,
    boot_pml4: 0,
    boot_tables_pages: 0,
    runtime_services: 0,
    smbios_table: 0,
    arch: ARCH_AARCH64_FDT,
    hartid: 0,
    dtb: 0,
};

/// Build the static BootInfo from the parsed FDT; DTB is the physical DTB
/// address QEMU passed in x0, STACK_TOP the link-time boot stack top.
pub fn build(
    mem: &crate::fdt::MemInfo,
    dtb: usize,
    stack_top: u64,
) -> &'static BootInfo {
    unsafe {
        let map = &mut *core::ptr::addr_of_mut!(MEMMAP);
        let mut n = 0usize;
        for i in 0..mem.mem_n {
            if n >= map.len() {
                break;
            }
            map[n] = MemoryDescriptor {
                type_: MEMORY_TYPE_CONVENTIONAL,
                _pad0: 0,
                physical_start: mem.mem[i].base,
                virtual_start: 0,
                number_of_pages: mem.mem[i].size / 4096,
                attribute: 0,
            };
            n += 1;
        }
        let bi = &mut *core::ptr::addr_of_mut!(BOOT_INFO);
        bi.memmap.ptr = map.as_ptr();
        bi.memmap.count = n;
        bi.stack_top = stack_top;
        bi.dtb = dtb as u64;
        &*core::ptr::addr_of!(BOOT_INFO)
    }
}
