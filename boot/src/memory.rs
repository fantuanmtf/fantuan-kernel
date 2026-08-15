//! EFI memory-map snapshot and boot-time page allocations.

use core::ffi::c_void;

use crate::console;
use crate::uefi::protocol::{MemoryDescriptor, SimpleTextOutput};
use crate::uefi::table::BootServices;
use crate::uefi::{ALLOCATE_MAX_ADDRESS, EFI_BUFFER_TOO_SMALL, EFI_LOADER_DATA, EFI_SUCCESS};

pub struct MemMapBuf {
    pub ptr: *mut c_void,
    pub capacity: usize,
    pub desc_size: usize,
    pub count: usize,
}

/// Snapshot the EFI memory map into a pooled buffer with headroom (the spec
/// allows the map to grow between calls).
pub fn snapshot(bs: &BootServices, con: *mut SimpleTextOutput) -> Option<MemMapBuf> {
    let mut map_key: usize = 0;
    let mut desc_size: usize = 0;
    let mut desc_version: u32 = 0;
    let mut map_size: usize = 0;
    let sts = (bs.get_memory_map)(
        &mut map_size,
        core::ptr::null_mut(),
        &mut map_key,
        &mut desc_size,
        &mut desc_version,
    );
    if sts != EFI_BUFFER_TOO_SMALL {
        let mut buf = [0u16; 256];
        let mut off = 0;
        console::write_ascii(&mut buf, &mut off, "ERROR: GetMemoryMap probe: sts=");
        console::write_hex64(&mut buf, &mut off, sts as u64);
        console::output_line(con, &mut buf, off);
        return None;
    }
    let capacity = map_size + desc_size * 4;
    let mut map_ptr: *mut c_void = core::ptr::null_mut();
    let sts = (bs.allocate_pool)(EFI_LOADER_DATA, capacity, &mut map_ptr);
    if sts != EFI_SUCCESS {
        console::println(con, "ERROR: AllocatePool(memory map) failed");
        return None;
    }
    let mut actual = capacity;
    let sts = (bs.get_memory_map)(
        &mut actual,
        map_ptr as *mut MemoryDescriptor,
        &mut map_key,
        &mut desc_size,
        &mut desc_version,
    );
    if sts != EFI_SUCCESS {
        console::println(con, "ERROR: GetMemoryMap failed");
        return None;
    }
    Some(MemMapBuf { ptr: map_ptr, capacity, desc_size, count: actual / desc_size })
}

/// Allocate the initial kernel stack BELOW the kernel image, so the firmware
/// can never hand out pages inside the kernel or its .bss.
pub fn allocate_stack(bs: &BootServices, con: *mut SimpleTextOutput, kernel_addr: u64) -> Option<u64> {
    let mut stack_addr: u64 = kernel_addr; // in: max address; out: allocated
    let sts = (bs.allocate_pages)(ALLOCATE_MAX_ADDRESS, EFI_LOADER_DATA, 16, &mut stack_addr);
    if sts != EFI_SUCCESS {
        console::println(con, "ERROR: stack allocation failed");
        return None;
    }
    Some(stack_addr + 16 * 4096)
}
