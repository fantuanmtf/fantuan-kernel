//! PCI configuration-space enumeration. DESIGN.md §2.1/§5.
//!
//! Module map:
//!   - read32(bus, dev, func, off) -> u32       — raw config-space dword read
//!   - PciDevice struct                         — bus/dev/fn + IDs + class + BAR0
//!   - list_display_devices() -> &[PciDevice]   — class 0x03 (display controllers)
//!   - find_storage_controllers() -> &[PciDevice] — class 0x01 (mass storage)
//!   - find_nvme() -> Option<PciDevice>         — class 0x01 subclass 0x08
//!   - find_ahci() -> Option<(u8,u8,u8,u64)>    — class 0x01 sub 0x06 progif 0x01

use crate::port::{inl, outl};
use core::sync::atomic::{AtomicBool, Ordering};

const CONFIG_ADDR: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

#[derive(Copy, Clone)]
pub struct PciDevice {
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub vendor: u16,
    pub device: u16,
    pub class: u8,
    pub subclass: u8,
    pub progif: u8,
    /// BAR0 — the NVMe register window (Task 8) uses this.
    #[allow(dead_code)]
    pub bar0: u64,
    /// BAR5 — the AHCI ABAR lives here (SATA controllers).
    pub bar5: u64,
}

// --- PCI config space access ---

pub fn read32(bus: u8, dev: u8, func: u8, off: u8) -> u32 {
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((dev as u32) << 11)
        | ((func as u32) << 8)
        | (off as u32 & 0xFC);
    unsafe {
        outl(CONFIG_ADDR, addr);
        inl(CONFIG_DATA)
    }
}

// --- Device scan + static catalog ---

static INIT: AtomicBool = AtomicBool::new(false);
static mut CATALOG: [Option<PciDevice>; 256] = [None; 256];
static mut DISPLAY: [PciDevice; 256] = [EMPTY_DEV; 256];
static mut DISPLAY_LEN: usize = 0;
static mut STORAGE: [PciDevice; 256] = [EMPTY_DEV; 256];
static mut STORAGE_LEN: usize = 0;

const EMPTY_DEV: PciDevice = PciDevice {
    bus: 0, dev: 0, func: 0, vendor: 0, device: 0,
    class: 0, subclass: 0, progif: 0, bar0: 0, bar5: 0,
};

fn scan_all() -> [Option<PciDevice>; 256] {
    let mut out: [Option<PciDevice>; 256] = [None; 256];
    let mut idx = 0usize;
    'outer: for bus in 0..=255u8 {
        for dev in 0..32u8 {
            for func in 0..8u8 {
                if idx >= 256 { break 'outer; }
                let vendev = read32(bus, dev, func, 0);
                let vendor = vendev as u16;
                if vendor == 0xFFFF { continue; }
                let device = (vendev >> 16) as u16;
                let c = read32(bus, dev, func, 8);
                let class = (c >> 24) as u8;
                let subclass = (c >> 16) as u8;
                let progif = (c >> 8) as u8;
                let bar0_low = read32(bus, dev, func, 0x10);
                let bar0_masked = (bar0_low & 0xFFFF_FFF0) as u64;
                let is_64 = (bar0_low & 0x01) == 0 && (bar0_low & 0x04) == 0x04;
                let bar0 = if is_64 {
                    let bar0_hi = read32(bus, dev, func, 0x14);
                    bar0_masked | ((bar0_hi as u64) << 32)
                } else {
                    bar0_masked
                };
                out[idx] = Some(PciDevice {
                    bus, dev, func, vendor, device,
                    class, subclass, progif, bar0,
                    bar5: (read32(bus, dev, func, 0x24) & 0xFFFF_FFF0) as u64,
                });
                idx += 1;
                if func == 0 && (read32(bus, dev, 0, 0x0C) & 0x0080_0000) == 0 {
                    break;
                }
            }
        }
    }
    out
}

unsafe fn init_catalog() {
    if INIT.load(Ordering::Acquire) { return; }
    let catalog = scan_all();
    let mut dlen = 0usize;
    let mut slen = 0usize;
    for i in 0..256 {
        if let Some(d) = catalog[i] {
            CATALOG[i] = Some(d);
            if d.class == 0x03 && dlen < 256 {
                DISPLAY[dlen] = d;
                dlen += 1;
            }
            if d.class == 0x01 && slen < 256 {
                STORAGE[slen] = d;
                slen += 1;
            }
        }
    }
    DISPLAY_LEN = dlen;
    STORAGE_LEN = slen;
    INIT.store(true, Ordering::Release);
}

// --- Public query helpers ---

pub fn list_display_devices() -> &'static [PciDevice] {
    unsafe {
        init_catalog();
        &DISPLAY[..DISPLAY_LEN]
    }
}

/// Storage-class catalog — the NVMe driver (Task 8) is the next consumer.
#[allow(dead_code)]
pub fn find_storage_controllers() -> &'static [PciDevice] {
    unsafe {
        init_catalog();
        &STORAGE[..STORAGE_LEN]
    }
}

#[allow(dead_code)]
pub fn find_nvme() -> Option<PciDevice> {
    unsafe {
        init_catalog();
        for i in 0..256 {
            if let Some(d) = CATALOG[i] {
                if d.class == 0x01 && d.subclass == 0x08 {
                    return Some(d);
                }
            }
        }
        None
    }
}

// find_ahci() retired with the block-device registry: drivers::init walks
// the storage catalog itself and lets each driver probe (AHCI or NVMe).
