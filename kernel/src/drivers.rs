//! The Rust side of the C driver boundary (DESIGN.md §5): exports for
//! rust_core.h plus the AHCI bring-up and verification flow.

use core::ffi::c_void;
use core::fmt::Write;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use crate::mm::frame;
use crate::mm::paging::phys_to_virt;
use crate::serial::{self, Serial};
use crate::tsc;

/// Bus/device of the storage controller we actually drive (AHCI or NVMe) —
/// the boot-repair NVRAM layer correlates whole-disk Boot#### entries
/// through it (M7.6).
static STORAGE_BDF: AtomicU32 = AtomicU32::new(u32::MAX);

pub fn storage_bdf() -> u32 {
    STORAGE_BDF.load(Ordering::Relaxed)
}

extern "C" {
    fn ahci_probe(abar: u64) -> i32;
    fn nvme_probe(bar0: u64) -> i32;
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
    /// Open a drive by index (driver ops registry); NULL when absent.
    fn blk_open(index: usize) -> *mut c_void;
    /// Driver name of the handle ("ahci"/"nvme").
    fn blk_name(dev: *mut c_void) -> *const u8;
    /// Driver-decoded identity (model/serial/sectors/ssd).
    fn blk_identity(dev: *mut c_void, out: *mut BlkIdentity) -> i32;
}

/// Generic identity as the C drivers report it (driver.h struct blk_identity).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BlkIdentity {
    pub model: [u8; 41],
    pub serial: [u8; 21],
    pub sectors: u64,
    pub ssd: i32,
}

impl BlkIdentity {
    pub const EMPTY: BlkIdentity = BlkIdentity {
        model: [0; 41],
        serial: [0; 21],
        sectors: 0,
        ssd: 0,
    };
}

/// Handle of the drive brought up by init() — diagnostics pass it to the
/// C driver ops (identity/SMART) instead of a NULL placeholder.
static DRIVE: AtomicUsize = AtomicUsize::new(0);

pub fn drive_handle() -> *mut c_void {
    DRIVE.load(Ordering::Relaxed) as *mut c_void
}

/// Name of the driver behind the active handle (for logs and dispatch).
pub fn drive_name() -> &'static str {
    let p = unsafe { blk_name(drive_handle()) };
    if p.is_null() {
        return "?";
    }
    let mut n = 0;
    while n < 16 && unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    core::str::from_utf8(unsafe { core::slice::from_raw_parts(p, n) }).unwrap_or("?")
}

/// Driver-decoded identity of the active drive.
pub fn drive_identity() -> Option<BlkIdentity> {
    let mut id = BlkIdentity::EMPTY;
    if unsafe { blk_identity(drive_handle(), &mut id) } == 0 {
        Some(id)
    } else {
        None
    }
}

// --- rust_core.h exports --------------------------------------------------

/// Log a NUL-terminated C string (bounded scan; serial channel).
#[no_mangle]
pub extern "C" fn k_log(s: *const u8) {
    if s.is_null() {
        return;
    }
    let mut len = 0;
    while unsafe { *s.add(len) } != 0 && len < 256 {
        len += 1;
    }
    let buf = unsafe { core::slice::from_raw_parts(s, len) };
    let _ = serial::write_locked(buf);
}

#[no_mangle]
pub extern "C" fn k_log_hex(v: u64) {
    // No trailing newline: C callers compose multi-part lines themselves.
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut buf = [0u8; 18];
    buf[0] = b'0';
    buf[1] = b'x';
    let mut n = 2;
    let mut started = false;
    for shift in (0..16).rev() {
        let nib = ((v >> (shift * 4)) & 0xF) as usize;
        if nib != 0 || started || shift == 0 {
            buf[n] = HEX[nib];
            n += 1;
            started = true;
        }
    }
    let _ = serial::write_locked(&buf[..n]);
}

#[no_mangle]
pub extern "C" fn k_phys_to_virt(phys: u64) -> u64 {
    phys_to_virt(phys)
}

/// One 4K DMA page for driver structures/buffers (see rust_core.h contract).
#[no_mangle]
pub extern "C" fn k_alloc_page(phys_out: *mut u64) -> u64 {
    let p = frame::get().alloc().expect("no frames for C driver");
    if !phys_out.is_null() {
        unsafe { *phys_out = p; }
    }
    phys_to_virt(p)
}

#[no_mangle]
pub extern "C" fn k_delay_ms(ms: u64) {
    tsc::sleep_ms(ms);
}

// --- bring-up + verification ------------------------------------------------

/// PCI-scan for AHCI, probe it, read LBA0 and verify the MBR signature
/// (0x55AA at offset 510) — the test disk is a GPT disk with a protective
/// MBR. Returns true when the full C-driver path worked.
pub fn init() -> bool {
    let mut s = Serial::new(serial::COM1);

    // Walk the storage-class catalog: AHCI (01/06/01) or NVMe (01/08). The
    // first controller that probes successfully wins and registers its ops.
    let mut probed = false;
    for d in crate::pci::find_storage_controllers() {
        if d.class != 0x01 {
            continue;
        }
        if d.subclass == 0x06 && d.progif == 0x01 {
            let _ = writeln!(s, "pci: AHCI at {:02x}:{:02x}.{}, ABAR {:#x}", d.bus, d.dev, d.func, d.bar5);
            if unsafe { ahci_probe(d.bar5) } == 0 {
                STORAGE_BDF.store(((d.bus as u32) << 8) | d.dev as u32, Ordering::Relaxed);
                probed = true;
                break;
            }
        } else if d.subclass == 0x08 {
            let _ = writeln!(s, "pci: NVMe at {:02x}:{:02x}.{}, BAR0 {:#x}", d.bus, d.dev, d.func, d.bar0);
            // 64-bit BARs live above 4 GiB: map the register window first.
            let mapped = crate::mm::paging::map_mmio(crate::mm::frame::get(), d.bar0, 16 * 1024);
            if mapped.is_none() {
                let _ = writeln!(s, "nvme: cannot map BAR0 {:#x} (out of frames?)", d.bar0);
                continue;
            }
            if unsafe { nvme_probe(d.bar0) } == 0 {
                STORAGE_BDF.store(((d.bus as u32) << 8) | d.dev as u32, Ordering::Relaxed);
                probed = true;
                break;
            }
        }
    }
    if !probed {
        let _ = writeln!(s, "pci: no usable storage controller");
        return false;
    }
    // M5.5: take the drive handle once; every later op goes through it.
    let handle = unsafe { blk_open(0) };
    if handle.is_null() {
        let _ = writeln!(s, "blk: blk_open(0) returned no handle");
        return false;
    }
    DRIVE.store(handle as usize, Ordering::Relaxed);
    let _ = writeln!(s, "blk: {} registered (drive 0)", drive_name());

    let mut sector = [0u8; 512];
    let rc = unsafe { blk_read(core::ptr::null_mut(), 0, sector.as_mut_ptr() as *mut c_void, 1) };
    if rc != 0 {
        let _ = writeln!(s, "ahci: LBA0 read failed");
        return false;
    }
    if sector[510] == 0x55 && sector[511] == 0xAA {
        let _ = writeln!(s, "ahci: LBA0 read ok: MBR signature verified");
        true
    } else {
        let _ = write!(s, "ahci: LBA0 mismatch, first bytes:");
        for b in &sector[..16] {
            let _ = write!(s, " {:02x}", b);
        }
        let _ = writeln!(s);
        false
    }
}
