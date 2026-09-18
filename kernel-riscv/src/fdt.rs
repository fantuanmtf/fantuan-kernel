//! FDT-lite (M9.1b): just enough flattened-device-tree parsing for boot —
//! memory nodes and the memreserve block. No general FDT library; only the
//! properties the frame allocator needs.
//!
//! Layout (Devicetree Specification v0.4): big-endian header at the DTB
//! address, a struct block of tokens, a strings block, and the memreserve
//! map (base/size u64 pairs terminated by 0/0).

pub const MAX_BLOCKS: usize = 8;

#[derive(Clone, Copy)]
pub struct Block {
    pub base: u64,
    pub size: u64,
}

pub struct MemInfo {
    pub mem: [Block; MAX_BLOCKS],
    pub mem_n: usize,
    pub reserved: [Block; MAX_BLOCKS],
    pub reserved_n: usize,
    pub totalsize: usize,
    /// /cpus timebase-frequency in Hz (10 MHz fallback for QEMU virt).
    pub timebase: u64,
    /// Root /model string (truncated).
    pub model: [u8; 64],
    pub model_len: usize,
    /// First CPU's riscv,isa string (truncated).
    pub isa: [u8; 48],
    pub isa_len: usize,
    /// Harts found under /cpus.
    pub harts: usize,
}

fn copy_str(dst: &mut [u8], src: &[u8]) -> usize {
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

/// Print the memory map and CPU report (bare-mode UART; call before paging).
pub fn print_info(mem: &MemInfo) {
    use crate::{put_bytes, put_dec, put_hex, puts};
    for i in 0..mem.mem_n {
        puts("fdt: memory ");
        put_hex(mem.mem[i].base);
        puts("..");
        put_hex(mem.mem[i].base + mem.mem[i].size);
        puts("\n");
    }
    puts("cpu: ");
    put_dec(mem.harts as u64);
    puts(" hart(s), isa=");
    put_bytes(&mem.isa[..mem.isa_len]);
    puts(", model=");
    put_bytes(&mem.model[..mem.model_len]);
    puts("\n");
}

/// Parse a DTB at `dtb` (physical address). Returns the memory/reserved
/// block lists; None on magic/length failure.
pub fn parse(dtb: usize) -> Option<MemInfo> {
    let d = unsafe { core::slice::from_raw_parts(dtb as *const u8, 512 * 1024) };
    if d.len() < 40 || be32(d, 0) != FDT_MAGIC {
        return None;
    }
    let totalsize = be32(d, 4) as usize;
    let off_struct = be32(d, 8) as usize;
    let off_strings = be32(d, 12) as usize;
    let off_rsvmap = be32(d, 16) as usize;
    if totalsize == 0 || totalsize > d.len() || off_struct >= totalsize || off_strings >= totalsize {
        return None;
    }
    let d = &d[..totalsize];

    let mut info = MemInfo {
        mem: [Block { base: 0, size: 0 }; MAX_BLOCKS],
        mem_n: 0,
        reserved: [Block { base: 0, size: 0 }; MAX_BLOCKS],
        reserved_n: 0,
        totalsize,
        timebase: 10_000_000,
        model: [0; 64],
        model_len: 0,
        isa: [0; 48],
        isa_len: 0,
        harts: 0,
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

    // Struct walk: root cell counts first, then memory nodes.
    let mut addr_cells = 2usize;
    let mut size_cells = 2usize;
    let mut depth = 0usize;
    let mut is_memory = false;
    let mut is_cpus = false;
    let mut p = off_struct;
    while p + 4 <= totalsize {
        let tok = be32(d, p);
        p += 4;
        match tok {
            FDT_BEGIN_NODE => {
                let name = cstr(d, p, 128);
                p += (name.len() + 1 + 3) & !3;
                depth += 1;
                // Root is depth 1; /memory@... nodes are its children.
                if depth == 2 {
                    is_memory = name.starts_with(b"memory");
                    is_cpus = name.starts_with(b"cpus");
                } else if depth == 3 && is_cpus && name.starts_with(b"cpu") {
                    info.harts += 1;
                }
            }
            FDT_END_NODE => {
                if depth == 2 {
                    is_memory = false;
                    is_cpus = false;
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
                if depth == 1 && name == b"#address-cells" && len >= 4 {
                    addr_cells = be32(d, p) as usize;
                } else if depth == 1 && name == b"#size-cells" && len >= 4 {
                    size_cells = be32(d, p) as usize;
                } else if is_cpus && name == b"timebase-frequency" && len >= 4 {
                    info.timebase = be32(d, p) as u64;
                } else if depth == 1 && name == b"model" {
                    info.model_len = copy_str(&mut info.model, &d[p..p + len]);
                } else if depth == 3 && is_cpus && name == b"riscv,isa" {
                    info.isa_len = copy_str(&mut info.isa, &d[p..p + len]);
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
