//! riscv64 driver glue (M9.4): the rust_core.h exports the C layer needs and
//! the virtio-mmio block bring-up. Storage is the only C driver on this arch
//! in v1 (no port I/O, so the AHCI/NVMe/i8042 C files do not link here).

use core::ffi::{c_char, c_void};

use crate::paging::{self, VIRTIO_MMIO_BASE};

extern "C" {
    /// Scan the virtio-mmio window at BASE (virtual); index of the first
    /// registered block device, or -1 when none is present.
    fn virtio_mmio_init(base: u64) -> i32;
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
    fn blk_open(index: usize) -> *mut c_void;
    fn blk_identity(dev: *mut c_void, out: *mut kernel_core::drv::BlkIdentity) -> i32;
}

fn drive_handle() -> *mut c_void {
    unsafe { blk_open(0) }
}

fn drive_name() -> &'static str {
    "virtio"
}

fn drive_identity() -> Option<kernel_core::drv::BlkIdentity> {
    let h = unsafe { blk_open(0) };
    if h.is_null() {
        return None;
    }
    let mut id = kernel_core::drv::BlkIdentity::EMPTY;
    if unsafe { blk_identity(h, &mut id) } != 0 {
        return None;
    }
    Some(id)
}

fn storage_bdf() -> u32 {
    u32::MAX // virtio-mmio has no PCI bus/device in the UEFI sense
}

#[no_mangle]
pub extern "C" fn k_log(s: *const c_char) {
    if s.is_null() {
        return;
    }
    let bytes = unsafe { core::ffi::CStr::from_ptr(s) }.to_bytes();
    kernel_core::log::put(bytes);
}

#[no_mangle]
pub extern "C" fn k_log_hex(v: u64) {
    crate::put_hex(v);
}

#[no_mangle]
pub extern "C" fn k_phys_to_virt(phys: u64) -> u64 {
    paging::phys_to_virt(phys)
}

#[no_mangle]
pub extern "C" fn k_alloc_page(phys_out: *mut u64) -> *mut c_void {
    let p = kernel_core::frame::get().alloc().expect("no frames for C driver");
    if !phys_out.is_null() {
        unsafe { *phys_out = p };
    }
    paging::phys_to_virt(p) as *mut c_void
}

/// Probe the virtio-mmio window; true when a block device registered and
/// its LBA0 passes the same MBR signature check the x86 AHCI path uses.
pub fn init_storage() -> bool {
    let base = paging::phys_to_virt(VIRTIO_MMIO_BASE);
    if unsafe { virtio_mmio_init(base) } < 0 {
        return false;
    }
    let mut sec = [0u8; 512];
    let rc = unsafe {
        blk_read(
            core::ptr::null_mut(),
            0,
            sec.as_mut_ptr() as *mut c_void,
            1,
        )
    };
    if rc != 0 || sec[510] != 0x55 || sec[511] != 0xAA {
        kernel_core::log::line("blk: LBA0 signature check FAILED");
        return false;
    }
    kernel_core::drv::set_ops(kernel_core::drv::DrvOps {
        drive_handle,
        drive_name,
        drive_identity,
        storage_bdf,
    });
    true
}
