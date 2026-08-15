//! SystemTable / BootServices table layouts (UEFI 2.x spec).
//!
//! Unused slots are usize placeholders — every pointer is 8 bytes on x86_64,
//! so the offsets stay correct; comments give the spec slot numbers.

use core::ffi::c_void;

use super::protocol::SimpleTextOutput;
use super::{Handle, Status, TableHeader};
use crate::uefi::guid::{ACPI2_GUID, Guid};

#[repr(C)]
pub struct ConfigurationTable {
    pub vendor_guid: Guid,
    pub vendor_table: *mut c_void,
}

#[repr(C)]
pub struct SystemTable {
    pub hdr: TableHeader,
    pub firmware_vendor: *mut u16,
    pub firmware_revision: u32,
    pub console_in_handle: Handle,
    pub con_in: usize,
    pub console_out_handle: Handle,
    pub con_out: *mut SimpleTextOutput,
    pub standard_error_handle: Handle,
    pub std_err: *mut SimpleTextOutput,
    pub runtime_services: usize,
    pub boot_services: *mut BootServices,
    pub number_of_table_entries: usize,
    pub configuration_table: *mut ConfigurationTable,
}

// Boot Services: header + 38 function-pointer slots (indices = spec slot numbers).
#[repr(C)]
pub struct BootServices {
    pub hdr: TableHeader,
    pub raise_tpl: usize,               // 1
    pub restore_tpl: usize,             // 2
    pub allocate_pages: extern "efiapi" fn(
        alloc_type: u32,
        memory_type: u32,
        pages: usize,
        memory: *mut u64,
    ) -> Status, // 3
    pub free_pages: usize,              // 4
    pub get_memory_map: extern "efiapi" fn(
        map_size: *mut usize,
        map: *mut super::protocol::MemoryDescriptor,
        map_key: *mut usize,
        desc_size: *mut usize,
        desc_version: *mut u32,
    ) -> Status, // 5
    pub allocate_pool: extern "efiapi" fn(
        pool_type: u32,
        size: usize,
        buffer: *mut *mut c_void,
    ) -> Status, // 6
    pub free_pool: usize,               // 7
    pub create_event: usize,            // 8
    pub set_timer: usize,               // 9
    pub wait_for_event: usize,          // 10
    pub signal_event: usize,            // 11
    pub close_event: usize,             // 12
    pub check_event: usize,             // 13
    pub install_protocol_interface: usize, // 14
    pub reinstall_protocol_interface: usize, // 15
    pub uninstall_protocol_interface: usize, // 16
    pub handle_protocol: usize,         // 17
    pub _reserved: usize,               // 18
    pub register_protocol_notify: usize, // 19
    pub locate_handle: usize,           // 20
    pub locate_device_path: usize,      // 21
    pub install_configuration_table: usize, // 22
    pub load_image: usize,              // 23
    pub start_image: usize,             // 24
    pub exit: usize,                    // 25
    pub unload_image: usize,            // 26
    pub exit_boot_services: extern "efiapi" fn(image_handle: Handle, map_key: usize) -> Status, // 27
    pub get_next_monotonic_count: usize, // 28
    pub stall: usize,                   // 29
    pub set_watchdog_timer: usize,      // 30
    pub connect_controller: usize,      // 31
    pub disconnect_controller: usize,   // 32
    pub open_protocol: usize,           // 33
    pub close_protocol: usize,          // 34
    pub open_protocol_information: usize, // 35
    pub protocols_per_handle: usize,    // 36
    pub locate_handle_buffer: usize,    // 37
    pub locate_protocol: extern "efiapi" fn(
        protocol: *const Guid,
        registration: *mut c_void,
        interface: *mut *mut c_void,
    ) -> Status, // 38
}

const _: () = assert!(core::mem::size_of::<BootServices>() == 24 + 38 * 8);

/// Find the ACPI 2.0 RSDP in the configuration table (SMP/ACPI entry point).
pub fn locate_rsdp(st: &SystemTable) -> u64 {
    let mut rsdp: u64 = 0;
    let n_tables = st.number_of_table_entries;
    let cfg = st.configuration_table;
    for i in 0..n_tables {
        let e = unsafe { &*cfg.add(i) };
        if e.vendor_guid.eq(&ACPI2_GUID) {
            rsdp = e.vendor_table as u64;
            break;
        }
    }
    rsdp
}
