//! FDT-lite: just enough flattened-device-tree parsing for boot — the memory
//! and memreserve blocks, the model string and the two nodes the ports need
//! beyond those (the CPU list and a serial port). No general FDT library.
//!
//! Layout (Devicetree Specification v0.4): big-endian header at the DTB
//! address, a struct block of tokens, a strings block, and the memreserve map
//! (base/size u64 pairs terminated by 0/0).
//!
//! Merged in 2026-10 from `kernel-riscv/src/fdt.rs` and
//! `kernel-aarch64/src/fdt.rs`, which had grown a byte-identical tokenizer,
//! `Block`, `copy_str`, `be32`/`be64`/`cstr` and memreserve loop each — and
//! had already drifted apart in the ways that matter: the riscv copy read a
//! fixed 512 KiB window and did not stop `copy_str` at the NUL, while the
//! aarch64 copy read the 40-byte header first and did. The aarch64 behaviour
//! is the one kept, so riscv inherits the fixes rather than needing them made
//! twice.
//!
//! Parsing a device tree is pure data decoding of an address the caller
//! supplies: no CSR, no MMIO, no page tables. That is why this belongs in
//! `kernel-core` next to `elf` and needs no `set_*` hook — the hook seam is
//! for I/O policy, and there is none here. Each port keeps its own
//! `print_info`, because the smoke suites assert the exact boot lines and the
//! two ports word them differently.

pub const MAX_BLOCKS: usize = 8;

/// Largest DTB accepted, as a sanity bound on a corrupt header.
const MAX_TOTALSIZE: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy)]
pub struct Block {
    pub base: u64,
    pub size: u64,
}

/// Everything both ports read out of the device tree. A property a given
/// machine does not carry simply leaves its field at the default, so the two
/// ports share one walker instead of one walker each.
pub struct Fdt {
    pub mem: [Block; MAX_BLOCKS],
    pub mem_n: usize,
    pub reserved: [Block; MAX_BLOCKS],
    pub reserved_n: usize,
    pub totalsize: usize,
    /// Root /model string, truncated, NUL-stripped.
    pub model: [u8; 64],
    pub model_len: usize,
    /// Children of /cpus (QEMU emits one node per hart/CPU).
    pub cpus: usize,
    /// Register base of the first pl011/serial/uart node, or 0 when the
    /// device tree has none for the port to fall back from.
    pub uart: u64,
    /// /cpus timebase-frequency in Hz; the QEMU `virt` value is the fallback.
    pub timebase: u64,
    /// First CPU's riscv,isa string, truncated (empty elsewhere).
    pub isa: [u8; 48],
    pub isa_len: usize,
}

impl Fdt {
    /// The memory blocks as a slice, for callers that just want to iterate.
    pub fn memory(&self) -> &[Block] {
        &self.mem[..self.mem_n]
    }
}

fn copy_str(dst: &mut [u8], src: &[u8]) -> usize {
    // Property values are NUL-terminated strings; stop at the terminator.
    let end = src.iter().position(|&b| b == 0).unwrap_or(src.len());
    let src = &src[..end];
    let n = src.len().min(dst.len());
    dst[..n].copy_from_slice(&src[..n]);
    n
}

const FDT_MAGIC: u32 = 0xd00d_feed;
const FDT_BEGIN_NODE: u32 = 1;
const FDT_END_NODE: u32 = 2;
const FDT_PROP: u32 = 3;
const FDT_NOP: u32 = 4;
const FDT_END: u32 = 9;

fn be32(d: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

fn be64(d: &[u8], off: usize) -> u64 {
    let mut v = 0u64;
    for i in 0..8 {
        v = (v << 8) | d[off + i] as u64;
    }
    v
}

fn cstr(d: &[u8], off: usize, max: usize) -> &[u8] {
    let end = (off + max).min(d.len());
    let mut n = off;
    while n < end && d[n] != 0 {
        n += 1;
    }
    &d[off..n]
}

/// Parse a DTB at `dtb` (physical address). Returns the blocks and the node
/// facts both ports need; None on magic/length failure or no memory node.
pub fn parse(dtb: usize) -> Option<Fdt> {
    // Read the 40-byte header first: QEMU pads the DTB (to 1 MiB on aarch64),
    // so a fixed window would either under-read or over-reserve.
    let hdr = unsafe { core::slice::from_raw_parts(dtb as *const u8, 40) };
    if be32(hdr, 0) != FDT_MAGIC {
        return None;
    }
    let totalsize = be32(hdr, 4) as usize;
    let off_struct = be32(hdr, 8) as usize;
    let off_strings = be32(hdr, 12) as usize;
    let off_rsvmap = be32(hdr, 16) as usize;
    if totalsize < 40 || totalsize > MAX_TOTALSIZE || off_struct >= totalsize || off_strings >= totalsize {
        return None;
    }
    let d = unsafe { core::slice::from_raw_parts(dtb as *const u8, totalsize) };

    let mut info = Fdt {
        mem: [Block { base: 0, size: 0 }; MAX_BLOCKS],
        mem_n: 0,
        reserved: [Block { base: 0, size: 0 }; MAX_BLOCKS],
        reserved_n: 0,
        totalsize,
        model: [0; 64],
        model_len: 0,
        cpus: 0,
        uart: 0,
        timebase: 10_000_000,
        isa: [0; 48],
        isa_len: 0,
    };

    // memreserve map: (base, size) pairs, terminated by (0, 0).
    let mut p = off_rsvmap;
    while p + 16 <= totalsize && info.reserved_n < MAX_BLOCKS {
        let base = be64(d, p);
        let size = be64(d, p + 8);
        p += 16;
        if base == 0 && size == 0 {
            break;
        }
        info.reserved[info.reserved_n] = Block { base, size };
        info.reserved_n += 1;
    }

    // Struct walk. Depth 1 is the root, depth 2 its children (/memory@...,
    // /cpus, the serial node), depth 3 their children (a cpu, a property the
    // ports want). The serial base is committed when its node closes, so the
    // `compatible` and `reg` properties may arrive in either order.
    let mut addr_cells = 2usize;
    let mut size_cells = 2usize;
    let mut depth = 0usize;
    let mut is_memory = false;
    let mut is_cpus = false;
    let mut is_serial = false;
    let mut serial_pl011 = false;
    let mut serial_base = 0u64;
    let mut p = off_struct;
    while p + 4 <= totalsize {
        let tok = be32(d, p);
        p += 4;
        match tok {
            FDT_BEGIN_NODE => {
                let name = cstr(d, p, 128);
                p += (name.len() + 1 + 3) & !3;
                depth += 1;
                if depth == 2 {
                    is_memory = name.starts_with(b"memory");
                    is_cpus = name.starts_with(b"cpus");
                    is_serial = name.starts_with(b"pl011")
                        || name.starts_with(b"serial")
                        || name.starts_with(b"uart");
                    serial_pl011 = false;
                    serial_base = 0;
                } else if depth == 3 && is_cpus && name.starts_with(b"cpu") {
                    info.cpus += 1;
                }
            }
            FDT_END_NODE => {
                if depth == 2 {
                    if is_serial && serial_pl011 && serial_base != 0 {
                        info.uart = serial_base;
                    }
                    is_memory = false;
                    is_cpus = false;
                    is_serial = false;
                }
                depth = depth.saturating_sub(1);
            }
            FDT_PROP => {
                if p + 8 > totalsize {
                    return None;
                }
                let len = be32(d, p) as usize;
                let nameoff = be32(d, p + 4) as usize;
                p += 8;
                if p + len > totalsize {
                    return None;
                }
                let name = cstr(d, off_strings + nameoff, 32);
                let value = &d[p..p + len];
                if depth == 1 && name == b"#address-cells" && len >= 4 {
                    addr_cells = be32(d, p) as usize;
                } else if depth == 1 && name == b"#size-cells" && len >= 4 {
                    size_cells = be32(d, p) as usize;
                } else if depth == 1 && name == b"model" {
                    info.model_len = copy_str(&mut info.model, value);
                } else if is_cpus && name == b"timebase-frequency" && len >= 4 {
                    info.timebase = be32(d, p) as u64;
                } else if depth == 3 && is_cpus && name == b"riscv,isa" {
                    info.isa_len = copy_str(&mut info.isa, value);
                } else if is_serial && name == b"compatible" {
                    serial_pl011 = value.windows(9).any(|w| w == b"arm,pl011");
                } else if is_serial && name == b"reg" && addr_cells <= 2 && len >= addr_cells * 4 {
                    let mut base = 0u64;
                    for i in 0..addr_cells {
                        base = (base << 32) | be32(d, p + i * 4) as u64;
                    }
                    serial_base = base;
                } else if is_memory && name == b"reg" {
                    let cells = addr_cells + size_cells;
                    if cells > 0 && cells <= 8 {
                        let mut off = p;
                        while off + cells * 4 <= p + len && info.mem_n < MAX_BLOCKS {
                            let mut base = 0u64;
                            for _ in 0..addr_cells {
                                base = (base << 32) | be32(d, off) as u64;
                                off += 4;
                            }
                            let mut size = 0u64;
                            for _ in 0..size_cells {
                                size = (size << 32) | be32(d, off) as u64;
                                off += 4;
                            }
                            if size > 0 {
                                info.mem[info.mem_n] = Block { base, size };
                                info.mem_n += 1;
                            }
                        }
                    }
                }
                p += (len + 3) & !3;
            }
            FDT_NOP => {}
            FDT_END => break,
            _ => return None,
        }
    }
    if info.mem_n == 0 {
        return None;
    }
    Some(info)
}
