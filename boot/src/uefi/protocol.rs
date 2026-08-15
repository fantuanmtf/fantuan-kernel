//! Protocol interface layouts: SimpleTextOutput (ConOut), GOP, SFS/File, and
//! the EFI_MEMORY_DESCRIPTOR used by the memory map.

use core::ffi::c_void;

use super::guid::{GOP_GUID, Guid};
use super::{Status, EFI_SUCCESS};

#[repr(C)]
pub struct SimpleTextOutput {
    pub reset: usize,                                                                           // 0
    pub output_string: extern "efiapi" fn(this: *mut SimpleTextOutput, s: *mut u16) -> Status, // 1
    pub test_string: usize,                                                                     // 2
    pub query_mode: usize,                                                                      // 3
    pub set_mode: usize,                                                                        // 4
    pub set_attribute: usize,                                                                   // 5
    pub clear_screen: extern "efiapi" fn(this: *mut SimpleTextOutput) -> Status,                // 6
}

#[repr(C)]
pub struct MemoryDescriptor {
    pub type_: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
}

#[repr(C)]
pub struct PixelBitmask {
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub reserved_mask: u32,
}

#[repr(C)]
pub struct GraphicsOutputModeInformation {
    pub version: u32,
    pub horizontal_resolution: u32,
    pub vertical_resolution: u32,
    pub pixel_format: u32,
    pub pixel_information: PixelBitmask,
    pub pixels_per_scan_line: u32,
}

#[repr(C)]
pub struct GraphicsOutputMode {
    pub max_mode: u32,
    pub mode: u32,
    pub info: *mut GraphicsOutputModeInformation,
    pub size_of_info: usize,
    pub frame_buffer_base: u64,
    pub frame_buffer_size: usize,
}

#[repr(C)]
pub struct GraphicsOutput {
    pub query_mode: usize,
    pub set_mode: usize,
    pub blt: usize,
    pub mode: *mut GraphicsOutputMode,
}

#[repr(C)]
pub struct SimpleFileSystem {
    pub revision: u64,
    pub open_volume: extern "efiapi" fn(this: *mut SimpleFileSystem, root: *mut *mut File) -> Status,
}

#[repr(C)]
pub struct File {
    pub revision: u64, // 0
    pub open: extern "efiapi" fn(
        this: *mut File,
        new_handle: *mut *mut File,
        file_name: *mut u16,
        open_mode: u64,
        attributes: u64,
    ) -> Status, // 1
    pub close: extern "efiapi" fn(this: *mut File) -> Status, // 2
    pub delete: usize, // 3
    pub read: extern "efiapi" fn(this: *mut File, buffer_size: *mut usize, buffer: *mut c_void) -> Status, // 4
    pub write: usize, // 5
    pub get_position: usize, // 6
    pub set_position: usize, // 7
    pub get_info: extern "efiapi" fn(
        this: *mut File,
        info_type: *mut Guid,
        info_size: *mut usize,
        info: *mut c_void,
    ) -> Status, // 8
}

/// The GOP query result (linear framebuffer).
#[derive(Clone, Copy)]
pub struct FrameBufferInfo {
    pub base: u64,
    pub size: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub format: u32,
}

impl FrameBufferInfo {
    pub const fn none() -> Self {
        Self { base: 0, size: 0, width: 0, height: 0, stride: 0, format: 0 }
    }
}

/// Query the GOP protocol for the linear framebuffer (DESIGN.md §4: this is
/// the rescue-system console). Returns None when headless.
pub fn locate_gop(bs: &super::table::BootServices) -> Option<FrameBufferInfo> {
    let mut gop: *mut c_void = core::ptr::null_mut();
    let sts = (bs.locate_protocol)(&GOP_GUID, core::ptr::null_mut(), &mut gop);
    if sts != EFI_SUCCESS {
        return None;
    }
    let gop = gop as *mut GraphicsOutput;
    let mode = unsafe { (*gop).mode };
    let info = unsafe { (*mode).info };
    Some(FrameBufferInfo {
        base: unsafe { (*mode).frame_buffer_base },
        size: unsafe { (*mode).frame_buffer_size } as u64,
        width: unsafe { (*info).horizontal_resolution },
        height: unsafe { (*info).vertical_resolution },
        stride: unsafe { (*info).pixels_per_scan_line },
        format: unsafe { (*info).pixel_format },
    })
}
