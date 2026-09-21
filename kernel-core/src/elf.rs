//! Minimal static-ELF loader (M4; shared since M9.3a): ET_EXEC, PT_LOAD only.
//!
//! Loads each loadable segment into fresh frames, copies the file bytes,
//! zeroes the rest of the segment, and maps the pages with the arch's
//! abstract protections (W^X from p_flags). Every header field is
//! range-checked against the image and kernel-half vaddrs are rejected, so a
//! malformed ELF fails the load instead of panicking or writing kernel page
//! tables.
//!
//! M10-4b3b: the loader accepts ELFCLASS32 (EM_386, the i686 kernel) as well
//! as ELFCLASS64. The two header layouts differ only in field widths and the
//! ELF32 program-header order (p_flags follows p_memsz); both are widened to
//! u64 here so the checks below are shared verbatim.

use fantuan_abi::PHYS_OFFSET;

use crate::frame;
use crate::mem::to_usize;
use crate::user::{self, Prot};

const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const PT_LOAD: u32 = 1;
const ELF32_HEADER: usize = 52;
const ELF64_HEADER: usize = 64;
/// Minimum program-header size per class (e_phentsize must be at least this).
const PHENT32_SIZE: usize = 32;
const PHENT64_SIZE: usize = 56;
const EM_386: u16 = 0x03;

struct Header {
    class32: bool,
    entry: u64,
    phoff: u64,
    phentsize: usize,
    phnum: usize,
}

struct Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_filesz: u64,
    p_memsz: u64,
}

fn u16_at(b: &[u8], o: usize) -> usize {
    u16::from_le_bytes([b[o], b[o + 1]]) as usize
}

fn u32_at(b: &[u8], o: usize) -> u64 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as u64
}

fn u64_at(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

/// Parse the class-specific ELF header fields. Returns None when the class
/// does not match the kernel's machine.
fn parse_header(elf: &[u8], machine: u16) -> Option<Header> {
    if elf[4] == 1 {
        if machine != EM_386 {
            return None;
        }
        Some(Header {
            class32: true,
            entry: u32_at(elf, 24),
            phoff: u32_at(elf, 28),
            phentsize: u16_at(elf, 42),
            phnum: u16_at(elf, 44),
        })
    } else if elf[4] == 2 {
        if machine == EM_386 {
            return None;
        }
        Some(Header {
            class32: false,
            entry: u64_at(elf, 24),
            phoff: u64_at(elf, 32),
            phentsize: u16_at(elf, 54),
            phnum: u16_at(elf, 56),
        })
    } else {
        None
    }
}

/// Read one program header; the caller guarantees `ph.len()` is at least the
/// class's minimum size.
fn parse_phdr(ph: &[u8], class32: bool) -> Phdr {
    if class32 {
        Phdr {
            p_type: u32_at(ph, 0) as u32,
            p_flags: u32_at(ph, 24) as u32,
            p_offset: u32_at(ph, 4),
            p_vaddr: u32_at(ph, 8),
            p_filesz: u32_at(ph, 16),
            p_memsz: u32_at(ph, 20),
        }
    } else {
        Phdr {
            p_type: u32_at(ph, 0) as u32,
            p_flags: u32_at(ph, 4) as u32,
            p_offset: u64_at(ph, 8),
            p_vaddr: u64_at(ph, 16),
            p_filesz: u64_at(ph, 32),
            p_memsz: u64_at(ph, 40),
        }
    }
}

/// Load an ET_EXEC image; returns (entry, root, image_end) where image_end is
/// the first address after the highest mapped page (P2 heap base derivation).
pub fn load_full(elf: &[u8]) -> Option<(u64, u64, u64)> {
    let ops = user::ops();
    if elf.len() < ELF32_HEADER || elf[0..4] != ELF_MAGIC {
        (ops.log)("elf: bad magic or too small");
        return None;
    }
    if elf[4] != 1 && elf[4] != 2 {
        (ops.log)("elf: bad ELF class");
        return None;
    }
    if elf[5] != 1 {
        (ops.log)("elf: not little-endian");
        return None;
    }
    if u16::from_le_bytes([elf[16], elf[17]]) != 2 {
        (ops.log)("elf: not ET_EXEC");
        return None;
    }
    if u16::from_le_bytes([elf[18], elf[19]]) != ops.machine {
        (ops.log)("elf: wrong machine");
        return None;
    }
    let min_header = if elf[4] == 1 { ELF32_HEADER } else { ELF64_HEADER };
    if elf.len() < min_header || (elf[4] == 1) != (ops.machine == EM_386) {
        (ops.log)("elf: class/machine mismatch");
        return None;
    }
    let hdr = parse_header(elf, ops.machine)?;
    let entry = hdr.entry;
    let phoff = hdr.phoff;
    let phentsize = hdr.phentsize;
    let phnum = hdr.phnum;

    // The program-header table must lie inside the image (checked math: the
    // values come from the file and must not wrap).
    let min_phent = if hdr.class32 { PHENT32_SIZE } else { PHENT64_SIZE };
    if phentsize < min_phent || phnum == 0 {
        (ops.log)("elf: bad program-header table");
        return None;
    }
    let ph_table_end = phoff.checked_add((phnum as u64).checked_mul(phentsize as u64)?)?;
    if ph_table_end > elf.len() as u64 {
        (ops.log)("elf: program headers outside the image");
        return None;
    }
    let phoff = to_usize(phoff)?;
    // The entry point is a user address too.
    if entry >= PHYS_OFFSET {
        (ops.log)("elf: entry in the kernel half");
        return None;
    }

    let root = (ops.new_root)();

    // Collect the loadable segments first. Overlapping PT_LOADs are legal
    // (e.g. a data segment sharing its first page with rodata), so every
    // distinct page must be allocated once, mapped with the union of the
    // covering segments' rights and filled with all of their bytes. Doing it
    // by address instead of a per-page table removes P1's fixed 64-page cap,
    // which a static bash (~190 pages) cannot fit.
    type Seg = (u64, u64, u64, u64, Prot);
    fn covers(seg: &Seg, page: u64) -> bool {
        let end = seg.0 + seg.3;
        seg.0 < page + 4096 && page < end
    }
    let mut segs: [Seg; 16] = [(0, 0, 0, 0, Prot::Ro); 16];
    let mut nsegs = 0usize;
    for i in 0..phnum {
        let ph = &elf[phoff + i * phentsize..][..phentsize];
        let ph = parse_phdr(ph, hdr.class32);
        if ph.p_type != PT_LOAD {
            continue;
        }
        // The segment must fit the user half and its file bytes must exist.
        if ph.p_vaddr.checked_add(ph.p_memsz)? > PHYS_OFFSET {
            (ops.log)("elf: PT_LOAD overflows the user half");
            return None;
        }
        if ph.p_offset.checked_add(ph.p_filesz)? > elf.len() as u64 || ph.p_filesz > ph.p_memsz {
            (ops.log)("elf: PT_LOAD file bytes outside the image");
            return None;
        }
        if nsegs >= segs.len() {
            (ops.log)("elf: too many PT_LOAD segments");
            return None;
        }
        // W^X from the program-header flags (PF_X = 1, PF_W = 2).
        let prot = match (ph.p_flags & 2 != 0, ph.p_flags & 1 != 0) {
            (false, false) => Prot::Ro,
            (true, false) => Prot::Rw,
            (false, true) => Prot::Rx,
            (true, true) => Prot::Rwx,
        };
        segs[nsegs] = (ph.p_vaddr, ph.p_offset, ph.p_filesz, ph.p_memsz, prot);
        nsegs += 1;
    }

    let mut image_end = 0u64;
    for si in 0..nsegs {
        let (vaddr, _offset, _filesz, memsz, prot) = segs[si];
        let seg_end = vaddr + memsz;
        let mut page = vaddr & !0xFFF;
        while page < seg_end {
            // A page first reached by an earlier segment was already fully
            // populated from every covering segment.
            if segs[..si].iter().any(|s| covers(s, page)) {
                page += 4096;
                continue;
            }
            let mut merged = prot;
            for s in segs[..nsegs].iter() {
                if covers(s, page) {
                    merged = merged.union(s.4);
                }
            }
            let Some(f) = frame::get().alloc() else {
                (ops.log)("elf: out of frames for a PT_LOAD page");
                return None;
            };
            let dst = (ops.phys_to_virt)(f) as *mut u8;
            unsafe { core::ptr::write_bytes(dst, 0, 4096) };
            for s in segs[..nsegs].iter() {
                if s.2 == 0 || !covers(s, page) {
                    continue;
                }
                let start = s.0.max(page);
                let page_off = to_usize(start - page)?;
                let seg_off = to_usize(start - s.0)?;
                if seg_off >= to_usize(s.2)? {
                    continue;
                }
                let copy = (to_usize(s.2)? - seg_off).min(4096 - page_off);
                let file_off = to_usize(s.1)?;
                let src = elf.get(file_off + seg_off..file_off + seg_off + copy)?;
                unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst.add(page_off), copy) };
            }
            (ops.map)(root, page, f, merged);
            image_end = image_end.max(page + 4096);
            page += 4096;
        }
    }

    Some((entry, root, image_end))
}

/// Compatibility wrapper (P1 callers): entry + root only.
pub fn load(elf: &[u8]) -> Option<(u64, u64)> {
    load_full(elf).map(|(e, r, _)| (e, r))
}
