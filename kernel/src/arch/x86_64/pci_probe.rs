//! PCI probe helpers for the M12-6 GPU report (report-only). Everything here
//! is configuration space: subsystem IDs, the standard write-1s BAR sizing
//! probe (the original value is restored immediately, no MMIO is touched)
//! and the PCIe capability walk. No device register is poked beyond reads of
//! identity/idle registers; no hang is possible because every loop is
//! bounded and every access is a single dword.
//!
//! Gated with `kconfig_graphics`: the GPU report is the only consumer.

use super::pci::{read16, read32, read8, write32};

/// PCIe capability ID (PCI Express Capability Structure).
pub const CAP_PCIE: u8 = 0x10;

/// (vendor, device) from config offset 0x2C (Subsystem Vendor/ID).
pub fn subsystem_ids(bus: u8, dev: u8, func: u8) -> (u16, u16) {
    let v = read32(bus, dev, func, 0x2C);
    (v as u16, (v >> 16) as u16)
}

/// Walk the capability list (bounded to 48 hops) and return the config-space
/// offset of capability `id`, or None when absent. The list is validated for
/// alignment and cycles; a malformed list terminates the walk.
pub fn capability(bus: u8, dev: u8, func: u8, id: u8) -> Option<u8> {
    let status = read16(bus, dev, func, 0x06);
    if status & (1 << 4) == 0 {
        return None;
    }
    let mut ptr = read8(bus, dev, func, 0x34) & 0xFC;
    for _ in 0..48 {
        if ptr < 0x40 {
            return None;
        }
        if read8(bus, dev, func, ptr) == id {
            return Some(ptr);
        }
        let next = read8(bus, dev, func, ptr + 1) & 0xFC;
        if next == 0 || next == ptr {
            return None;
        }
        ptr = next;
    }
    None
}

#[derive(Copy, Clone)]
pub struct Bar {
    pub index: u8,
    pub base: u64,
    pub size: u64,
    pub is_io: bool,
    pub is_64: bool,
}

/// Size one BAR with the standard write-1s config probe and restore the
/// original value. Returns None for an unimplemented (zero) BAR.
pub fn bar(bus: u8, dev: u8, func: u8, index: u8) -> Option<Bar> {
    if index > 5 {
        return None;
    }
    let off = 0x10 + index * 4;
    let orig_lo = read32(bus, dev, func, off);
    if orig_lo == 0 {
        return None;
    }
    if orig_lo & 1 != 0 {
        // I/O space BAR: bits 1:0 are reserved, size in bits 31:2.
        write32(bus, dev, func, off, 0xFFFF_FFFF);
        let mask = read32(bus, dev, func, off) & !0x3;
        write32(bus, dev, func, off, orig_lo);
        let size = (!mask).wrapping_add(1) as u64;
        if size == 0 {
            return None;
        }
        return Some(Bar {
            index,
            base: (orig_lo & !0x3) as u64,
            size,
            is_io: true,
            is_64: false,
        });
    }
    let is_64 = orig_lo & 0x06 == 0x04;
    let orig_hi = if is_64 { read32(bus, dev, func, off + 4) } else { 0 };
    write32(bus, dev, func, off, 0xFFFF_FFFF);
    if is_64 {
        write32(bus, dev, func, off + 4, 0xFFFF_FFFF);
    }
    let mask_lo = read32(bus, dev, func, off);
    let mask_hi = if is_64 { read32(bus, dev, func, off + 4) } else { 0xFFFF_FFFF };
    write32(bus, dev, func, off, orig_lo);
    if is_64 {
        write32(bus, dev, func, off + 4, orig_hi);
    }
    let mask = (mask_lo as u64 & !0xF) | ((mask_hi as u64) << 32);
    let size = (!mask).wrapping_add(1);
    if size == 0 {
        return None;
    }
    let base = (orig_lo as u64 & !0xF) | ((orig_hi as u64) << 32);
    Some(Bar { index, base, size, is_io: false, is_64 })
}

#[derive(Copy, Clone)]
pub struct PcieLink {
    pub speed: u8,
    pub width: u16,
    pub max_speed: u8,
    pub max_width: u16,
}

/// Current and maximum link speed/width from the PCIe capability (Link
/// Capabilities at +0x0C) and the Link Status register (at +0x12).
pub fn pcie_link(bus: u8, dev: u8, func: u8) -> Option<PcieLink> {
    let ptr = capability(bus, dev, func, CAP_PCIE)?;
    let cap = read32(bus, dev, func, ptr + 0x0C);
    let status = read16(bus, dev, func, ptr + 0x12);
    Some(PcieLink {
        speed: (status & 0xF) as u8,
        width: (status >> 4) & 0x3F,
        max_speed: (cap & 0xF) as u8,
        max_width: ((cap >> 4) & 0x3F) as u16,
    })
}

/// PCIe link-speed encoding -> the GT/s string (PCIe 1.0..6.0).
pub fn speed_name(speed: u8) -> &'static str {
    match speed {
        1 => "2.5GT/s",
        2 => "5GT/s",
        3 => "8GT/s",
        4 => "16GT/s",
        5 => "32GT/s",
        6 => "64GT/s",
        _ => "unknown",
    }
}
