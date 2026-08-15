//! UEFI base types and constants. Hand-rolled against the UEFI 2.x spec;
//! zero external crates.
//!
//! Submodules: guid (identifiers) · table (SystemTable/BootServices) ·
//! protocol (GOP, SFS, ConOut interface layouts).

pub mod guid;
pub mod protocol;
pub mod table;

use core::ffi::c_void;

pub type Handle = *mut c_void;
pub type Status = usize;

pub const EFI_SUCCESS: Status = 0;
// EFI error codes have the high bit set (EFIERR(n) = 0x8000...0000 | n).
pub const EFI_LOAD_ERROR: Status = 0x8000_0000_0000_0001;
pub const EFI_BUFFER_TOO_SMALL: Status = 0x8000_0000_0000_0005;

pub const EFI_SYSTEM_TABLE_SIGNATURE: u64 = 0x5453_5953_2049_4249; // "IBI SYST"
pub const EFI_BOOT_SERVICES_SIGNATURE: u64 = 0x5652_4553_544f_4f42; // "BOOTSERV"

pub const EFI_LOADER_DATA: u32 = 2;
pub const ALLOCATE_MAX_ADDRESS: u32 = 1;
pub const EFI_FILE_MODE_READ: u64 = 1;

#[repr(C)]
pub struct TableHeader {
    pub signature: u64,
    pub revision: u32,
    pub header_size: u32,
    pub crc32: u32,
    pub reserved: u32,
}
