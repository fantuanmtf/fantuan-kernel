//! The Rust side of the C driver boundary (DESIGN.md §5): exports for
//! rust_core.h plus the AHCI bring-up and verification flow.

use core::ffi::c_void;
use core::fmt::Write;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::mm::frame;
use crate::mm::paging::phys_to_virt;
use crate::serial::{self, Serial};
use crate::tsc;

/// Bus/device of the AHCI controller we actually drive — the boot-repair
/// NVRAM layer correlates whole-disk Boot#### entries through it (M7.6).
static AHCI_BDF: AtomicU32 = AtomicU32::new(u32::MAX);

pub fn ahci_bdf() -> u32 {
    AHCI_BDF.load(Ordering::Relaxed)
}

extern "C" {
    fn ahci_probe(abar: u64) -> i32;
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
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
    let Some((bus, dev, _func, abar)) = crate::pci::find_ahci() else {
        let _ = writeln!(s, "pci: no AHCI controller found");
        return false;
    };
    AHCI_BDF.store(((bus as u32) << 8) | dev as u32, Ordering::Relaxed);
    let _ = writeln!(s, "pci: AHCI at {:02x}:{:02x}.0, ABAR {:#x}", bus, dev, abar);

    if unsafe { ahci_probe(abar) } != 0 {
        let _ = writeln!(s, "ahci: probe failed");
        return false;
    }

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
