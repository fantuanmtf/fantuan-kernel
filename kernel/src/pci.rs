//! Minimal PCI configuration-space enumeration — Rust core arch code, not a
//! driver (DESIGN.md §2.1/§5). Finds the AHCI controller and hands its ABAR
//! to the C probe; the C layer never touches PCI config space itself.

use crate::port::{inl, outl};

const CONFIG_ADDR: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

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

/// Find the first AHCI SATA controller (class 0x01, subclass 0x06,
/// prog-if 0x01). Returns (bus, dev, func, ABAR physical address).
pub fn find_ahci() -> Option<(u8, u8, u8, u64)> {
    for bus in 0..=255u8 {
        for dev in 0..32u8 {
            let vendor = read32(bus, dev, 0, 0);
            if vendor == 0xFFFF_FFFF {
                continue; // no device at this slot
            }
            let c = read32(bus, dev, 0, 8);
            let class = (c >> 24) as u8;
            let subclass = (c >> 16) as u8;
            let progif = (c >> 8) as u8;
            if class == 0x01 && subclass == 0x06 && progif == 0x01 {
                let bar5 = read32(bus, dev, 0, 0x24);
                let abar = (bar5 & 0xFFFF_FFF0) as u64;
                return Some((bus, dev, 0, abar));
            }
        }
    }
    None
}
