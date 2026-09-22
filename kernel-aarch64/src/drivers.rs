//! C-driver stubs for the direct-FDT path (M11 R9a). The shared VFS and the
//! rescue-profile disk diagnostics are compiled into the kernel and resolve
//! their block calls through extern symbols the arch supplies; aarch64 has
//! no storage transport yet (R9b), so these return honest failures and every
//! path degrades to "no drive" instead of breaking the link.

use core::ffi::c_void;

use kernel_core::drv::BlkIdentity;

/// No block registry on this machine: the CONFIG_IMAGER core resolves its
/// `blk_open`/`blk_identity` seams here and finds no device.
#[no_mangle]
pub extern "C" fn blk_open(_index: usize) -> *mut c_void {
    core::ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn blk_name(_dev: *mut c_void) -> *const u8 {
    b"?\0".as_ptr()
}

#[no_mangle]
pub extern "C" fn blk_identity(_dev: *mut c_void, _out: *mut BlkIdentity) -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn blk_read(_dev: *mut c_void, _lba: u64, _buf: *mut c_void, _sectors: usize) -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn blk_write(
    _dev: *mut c_void,
    _lba: u64,
    _buf: *const c_void,
    _sectors: usize,
) -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn blk_smart_read_data(_dev: *mut c_void, _out: *mut c_void) -> i32 {
    -1
}

#[no_mangle]
pub extern "C" fn blk_smart_read_log(
    _dev: *mut c_void,
    _page: u8,
    _buf: *mut c_void,
    _sectors: usize,
) -> i32 {
    -1
}
