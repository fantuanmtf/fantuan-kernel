//! BOOT_INFO construction and the ExitBootServices + jump sequence.

use core::ffi::c_void;

use crate::console;
use crate::memory::MemMapBuf;
use crate::paging::{self, TablePages};
use crate::serial;
use crate::uefi::protocol::{FrameBufferInfo, MemoryDescriptor, SimpleTextOutput};
use crate::uefi::table::BootServices;
use crate::uefi::{Handle, EFI_BUFFER_TOO_SMALL, EFI_LOADER_DATA, EFI_SUCCESS};
use fantuan_abi::{BootInfo, FrameBuffer, MemMap, BOOT_MAGIC, PHYS_OFFSET};

// Lives in the EFI app's own image (.data/.bss), not on the firmware stack:
// the app image stays in memory forever, so the kernel can read it safely
// after the jump. Written just before the jump.
static mut BOOT_INFO: BootInfo = BootInfo {
    magic: 0,
    version: 0,
    memmap: MemMap { ptr: core::ptr::null(), count: 0, desc_size: 0 },
    fb: FrameBuffer { base: 0, size: 0, width: 0, height: 0, stride: 0, format: 0 },
    rsdp: 0,
    kernel_base: 0,
    stack_top: 0,
    caps: 0,
    boot_pml4: 0,
    boot_tables_pages: 0,
    runtime_services: 0,
    smbios_table: 0,
};

/// ExitBootServices with the classic map-key retry, fill BOOT_INFO, then jump
/// to the kernel. Does not return.
pub fn exit_and_jump(
    bs: &BootServices,
    con: *mut SimpleTextOutput,
    image_handle: Handle,
    fb: FrameBufferInfo,
    rsdp: u64,
    stack_top: u64,
    kernel_addr: u64,
    map: &mut MemMapBuf,
    tables: &TablePages,
    runtime_services: u64,
    smbios_table: u64,
) -> ! {
    console::println(con, "exiting boot services...");
    let mut attempts = 0;
    loop {
        let mut k: usize = 0;
        // Always pass the FULL allocated capacity: the map may grow between
        // calls (our own stack allocation just changed the key). GetMemoryMap
        // updates sz with the actual size on return.
        let mut sz = map.capacity;
        let mut dsz = map.desc_size;
        let mut dv: u32 = 0;
        let sts = (bs.get_memory_map)(
            &mut sz,
            map.ptr as *mut MemoryDescriptor,
            &mut k,
            &mut dsz,
            &mut dv,
        );
        if sts == EFI_BUFFER_TOO_SMALL && attempts < 8 {
            // The map outgrew the snapshot's headroom (OVMF can add
            // descriptors while retrying ExitBootServices). Boot Services are
            // still alive on this path, so probe the size and grow the buffer.
            if !grow_map(bs, con, map) {
                loop_halt();
            }
            attempts += 1;
            continue;
        }
        if sts != EFI_SUCCESS {
            let mut buf = [0u16; 256];
            let mut off = 0;
            console::write_ascii(&mut buf, &mut off, "ERROR: GetMemoryMap failed sts=");
            console::write_hex64(&mut buf, &mut off, sts as u64);
            console::output_line(con, &mut buf, off);
            loop_halt();
        }
        let sts = (bs.exit_boot_services)(image_handle, k);
        if sts == EFI_SUCCESS {
            map.count = sz / dsz;
            break;
        }
        attempts += 1;
        if attempts > 8 {
            console::println(con, "ERROR: ExitBootServices failed repeatedly");
            loop_halt();
        }
    }

    // Firmware is gone: ConOut and Boot Services are dead from here on.
    // Switch to our own serial output, enable paging, report the handoff.
    serial::init();
    serial::line("[boot] exit boot services: ok");
    paging::enable(tables);
    serial::line("[boot] paging: identity 4GiB + PHYS_OFFSET alias");
    serial::line("[boot] pml4 @");
    serial::hex(tables.pml4);
    serial::line("[boot] BootInfo @");
    serial::hex(&raw const BOOT_INFO as usize as u64);
    serial::line("[boot] jumping to kernel...");

    unsafe {
        BOOT_INFO = BootInfo {
            magic: BOOT_MAGIC,
            version: 1,
            memmap: MemMap {
                ptr: map.ptr as *const fantuan_abi::MemoryDescriptor,
                count: map.count,
                desc_size: map.desc_size,
            },
            fb: FrameBuffer {
                base: fb.base,
                size: fb.size,
                width: fb.width,
                height: fb.height,
                stride: fb.stride,
                format: fb.format,
            },
            rsdp,
            kernel_base: kernel_addr,
            stack_top,
            caps: 0,
            boot_pml4: tables.pml4,
            boot_tables_pages: tables.pages,
            runtime_services,
            smbios_table,
        };
    }

    type KernelEntry = extern "sysv64" fn(boot_info: *const BootInfo, stack_top: u64) -> !;
    let entry: KernelEntry = unsafe { core::mem::transmute((PHYS_OFFSET + kernel_addr) as *const ()) };
    entry(&raw const BOOT_INFO, stack_top);
}

/// Grow the memory-map buffer after EFI_BUFFER_TOO_SMALL: re-probe the
/// required size, allocate a larger pool block (with headroom for the next
/// change), and update MAP. The old buffer is deliberately not freed — that
/// would itself change the map again.
fn grow_map(bs: &BootServices, con: *mut SimpleTextOutput, map: &mut MemMapBuf) -> bool {
    let mut need: usize = 0;
    let mut key: usize = 0;
    let mut dsz: usize = map.desc_size;
    let mut dv: u32 = 0;
    let sts = (bs.get_memory_map)(&mut need, core::ptr::null_mut(), &mut key, &mut dsz, &mut dv);
    if sts != EFI_BUFFER_TOO_SMALL || dsz == 0 || need == 0 {
        console::println(con, "ERROR: GetMemoryMap size probe failed");
        return false;
    }
    let capacity = need + dsz * 8;
    let mut ptr: *mut c_void = core::ptr::null_mut();
    if (bs.allocate_pool)(EFI_LOADER_DATA, capacity, &mut ptr) != EFI_SUCCESS || ptr.is_null() {
        console::println(con, "ERROR: AllocatePool(grow memory map) failed");
        return false;
    }
    map.ptr = ptr;
    map.capacity = capacity;
    map.desc_size = dsz;
    true
}

fn loop_halt() -> ! {
    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)) }
    }
}
