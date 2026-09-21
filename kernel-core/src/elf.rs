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

    // Track already-mapped pages: two segments may share a 4K page (e.g.
    // .rodata and .data), and the second must reuse the first's frame — a
    // fresh frame would shadow the earlier content. The entry also carries
    // the accumulated protection so a shared page gets the union of rights
    // (W if any segment writes it; executable if any segment executes it).
    let mut mapped: [(u64, u64, Prot); 64] = [(0, 0, Prot::Ro); 64];
    let mut mapped_n = 0;
    let mut image_end = 0u64;

    for i in 0..phnum {
        let ph = &elf[phoff + i * phentsize..][..phentsize];
        let ph = parse_phdr(ph, hdr.class32);
        if ph.p_type != PT_LOAD {
            continue;
        }
        let p_offset = ph.p_offset;
        let p_vaddr = ph.p_vaddr;
        let p_filesz = ph.p_filesz;
        let p_memsz = ph.p_memsz;
        // The segment must fit the user half and its file bytes must exist.
        if p_vaddr.checked_add(p_memsz)? > PHYS_OFFSET {
            (ops.log)("elf: PT_LOAD overflows the user half");
            return None;
        }
        if p_offset.checked_add(p_filesz)? > elf.len() as u64 || p_filesz > p_memsz {
            (ops.log)("elf: PT_LOAD file bytes outside the image");
            return None;
        }
        // W^X from the program-header flags (PF_X = 1, PF_W = 2).
        let prot = match (ph.p_flags & 2 != 0, ph.p_flags & 1 != 0) {
            (false, false) => Prot::Ro,
            (true, false) => Prot::Rw,
            (false, true) => Prot::Rx,
            (true, true) => Prot::Rwx,
        };
        let p_offset = to_usize(p_offset)?;
        let p_filesz = to_usize(p_filesz)?;
        let page_start = p_vaddr & !0xFFF;
        let seg_end = p_vaddr + p_memsz;
        let mut page = page_start;
        while page < seg_end {
            let head = if page == page_start { (p_vaddr - page_start) as usize } else { 0 };
            let f = match mapped[..mapped_n].iter().position(|e| e.0 == page) {
                Some(idx) => {
                    // Shared page: union the protections.
                    let phys = mapped[idx].1;
                    let merged = mapped[idx].2.union(prot);
                    mapped[idx].2 = merged;
                    (ops.map)(root, page, phys, merged);
                    phys
                }
                None => {
                    let f = frame::get().alloc()?;
                    if mapped_n >= mapped.len() {
                        // More distinct pages than the tracking table holds:
                        // refuse rather than map an untracked page (which a
                        // later segment could double-allocate).
                        return None;
                    }
                    mapped[mapped_n] = (page, f, prot);
                    mapped_n += 1;
                    (ops.map)(root, page, f, prot);
                    let dst = (ops.phys_to_virt)(f) as *mut u8;
                    unsafe { core::ptr::write_bytes(dst, 0, 4096) };
                    f
                }
            };
            let seg_off = to_usize((page + head as u64) - p_vaddr)?; // offset into the segment
            let dst = (ops.phys_to_virt)(f) as *mut u8;
            if seg_off < p_filesz {
                let copy = (p_filesz - seg_off).min(4096 - head);
                let src = elf.get(p_offset + seg_off..p_offset + seg_off + copy)?;
                unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst.add(head), copy) };
            }
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
