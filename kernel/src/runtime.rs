//! UEFI Runtime Services (M7.5): survive ExitBootServices, called with
//! PHYSICAL addresses (no SetVirtualAddressMap — the 4 GiB identity map
//! covers everything). Read-only usage in v1: GetVariable /
//! GetNextVariableName / GetTime.

use core::ffi::c_void;

pub type Status = usize;

pub const RT_SUCCESS: Status = 0;

pub const RT_SIGNATURE: u64 = 0x5652_4553_544e_5552; // "RUNT SERV"

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Guid {
    pub a: u32,
    pub b: u16,
    pub c: u16,
    pub d: [u8; 8],
}

/// EFI_GLOBAL_VARIABLE vendor GUID.
pub const GLOBAL_GUID: Guid = Guid {
    a: 0x8be4_df61,
    b: 0x93ca,
    c: 0x11d2,
    d: [0xaa, 0x0d, 0x00, 0xe0, 0x98, 0x03, 0x2b, 0x8c],
};

#[repr(C)]
struct TableHeader {
    signature: u64,
    revision: u32,
    header_size: u32,
    crc32: u32,
    reserved: u32,
}

// Runtime Services: header + function pointers (spec slot numbers).
#[repr(C)]
pub struct RuntimeServices {
    hdr: TableHeader,
    pub get_time: extern "efiapi" fn(
        time: *mut Time,
        capabilities: *mut u64,
    ) -> Status, // 0
    _set_time: usize,                 // 1
    _get_wakeup: usize,               // 2
    _set_wakeup: usize,               // 3
    _set_vam: usize,                  // 4
    _convert_pointer: usize,          // 5
    pub get_variable: extern "efiapi" fn(
        name: *mut u16,
        vendor_guid: *mut Guid,
        attributes: *mut u32,
        data_size: *mut usize,
        data: *mut c_void,
    ) -> Status, // 6
    pub get_next_variable_name: extern "efiapi" fn(
        name_size: *mut usize,
        name: *mut u16,
        vendor_guid: *mut Guid,
    ) -> Status, // 7
    pub set_variable: extern "efiapi" fn(
        name: *mut u16,
        vendor_guid: *mut Guid,
        attributes: u32,
        data_size: usize,
        data: *mut c_void,
    ) -> Status, // 8
}

#[repr(C)]
pub struct Time {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub _pad1: u8,
    pub nanosecond: u32,
    pub timezone: i16,
    pub daylight: u8,
    pub _pad2: u8,
}

pub struct Runtime {
    table: *mut RuntimeServices,
}

impl Runtime {
    /// table_phys: the BootInfo.runtime_services physical address.
    pub fn new(table_phys: u64) -> Option<Runtime> {
        if table_phys == 0 {
            return None;
        }
        let table = table_phys as *mut RuntimeServices;
        if unsafe { (*table).hdr.signature } != RT_SIGNATURE {
            return None;
        }
        Some(Runtime { table })
    }

    /// Read a variable; returns the number of bytes copied (≤ buf.len()).
    pub fn get_variable(&self, name: &[u16], guid: &Guid, buf: &mut [u8]) -> Option<usize> {
        let mut name_buf = [0u16; 64];
        for (i, c) in name.iter().enumerate() {
            if i + 1 < name_buf.len() {
                name_buf[i] = *c;
            }
        }
        let mut vendor = *guid;
        let mut attrs: u32 = 0;
        let mut size = buf.len();
        let sts = unsafe {
            ((*self.table).get_variable)(
                name_buf.as_mut_ptr(),
                &mut vendor,
                &mut attrs,
                &mut size,
                buf.as_mut_ptr() as *mut c_void,
            )
        };
        if sts == RT_SUCCESS {
            Some(size)
        } else {
            None
        }
    }

    /// Enumerate variable names; empty name starts the iteration. The caller
    /// provides a name buffer (UTF-16, NUL-terminated on return). Returns the
    /// variable's vendor GUID.
    pub fn next_variable(&self, name: &mut [u16], vendor: &mut Guid) -> bool {
        let mut size = name.len() * 2;
        let sts = unsafe {
            ((*self.table).get_next_variable_name)(&mut size, name.as_mut_ptr(), vendor)
        };
        sts == RT_SUCCESS
    }

    /// Write a variable (attributes NV|BS|RT). Used by the NVRAM self-test.
    pub fn set_variable(&self, name: &[u16], guid: &Guid, data: &[u8]) -> Status {
        let mut name_buf = [0u16; 64];
        for (i, c) in name.iter().enumerate() {
            if i + 1 < name_buf.len() {
                name_buf[i] = *c;
            }
        }
        let mut vendor = *guid;
        unsafe {
            ((*self.table).set_variable)(
                name_buf.as_mut_ptr(),
                &mut vendor,
                0x7, // NV | BS | RT — the runtime-create requirement
                data.len(),
                data.as_ptr() as *mut c_void,
            )
        }
    }

    pub fn get_time(&self) -> Option<Time> {
        let mut t = Time {
            year: 0, month: 0, day: 0, hour: 0, minute: 0, second: 0,
            _pad1: 0, nanosecond: 0, timezone: 0, daylight: 0, _pad2: 0,
        };
        let sts = unsafe { ((*self.table).get_time)(&mut t, core::ptr::null_mut()) };
        if sts == RT_SUCCESS {
            Some(t)
        } else {
            None
        }
    }
}
