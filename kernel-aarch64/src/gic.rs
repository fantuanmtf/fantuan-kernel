//! GICv2 bring-up for QEMU `virt` (M11 R9a): distributor at 0x0800_0000,
//! CPU interface at 0x0801_0000. Both are hardcoded to the QEMU virt map
//! (documented in fdt.rs); only PPI 30 (the EL1 physical timer) is enabled.
//! `run.sh` passes `-machine virt,gic-version=2` so the default GICv3 build
//! of newer QEMU cannot silently move the CPU interface.

const GICD: usize = 0x0800_0000;
const GICC: usize = 0x0801_0000;

/// CPU interface registers.
const GICC_CTLR: usize = 0x0000;
const GICC_PMR: usize = 0x0004;
const GICC_IAR: usize = 0x000C;
const GICC_EOIR: usize = 0x0010;
/// Distributor registers.
const GICD_CTLR: usize = 0x0000;
const GICD_ISENABLER0: usize = 0x0100;
const GICD_IPRIORITYR: usize = 0x0400;

/// EL1 physical timer PPI (non-secure; QEMU has no EL3 by default).
pub const TIMER_IRQ: u32 = 30;

fn rd(base: usize, off: usize) -> u32 {
    unsafe { core::ptr::read_volatile((base + off) as *const u32) }
}

fn wr(base: usize, off: usize, v: u32) {
    unsafe { core::ptr::write_volatile((base + off) as *mut u32, v) }
}

/// Enable PPI N in the distributor (bank 0 covers IRQs 0..31).
pub fn enable_irq(n: u32) {
    wr(GICD, GICD_ISENABLER0, 1 << n);
}

/// Priority for one PPI (byte lane).
fn set_priority(n: u32, prio: u8) {
    let off = GICD_IPRIORITYR + (n as usize & !3);
    let shift = (n as usize & 3) * 8;
    let old = rd(GICD, off) & !(0xFF << shift);
    wr(GICD, off, old | ((prio as u32) << shift));
}

/// Bring up the CPU interface first, then the distributor, then IRQ 30.
pub fn init() {
    wr(GICC, GICC_PMR, 0xFF);
    wr(GICC, GICC_CTLR, 1);
    set_priority(TIMER_IRQ, 0xA0);
    enable_irq(TIMER_IRQ);
    wr(GICD, GICD_CTLR, 1);
}

/// Acknowledge the highest-priority pending interrupt.
pub fn ack() -> u32 {
    rd(GICC, GICC_IAR)
}

/// Signal end of interrupt (after the handler re-arms the source).
pub fn eoi(id: u32) {
    wr(GICC, GICC_EOIR, id);
}
