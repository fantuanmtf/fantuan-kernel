//! Minimal static-ELF loader (M4): ET_EXEC, x86_64, PT_LOAD only.
//!
//! Loads each loadable segment into fresh frames, copies the file bytes,
//! zeroes the rest of the segment, and maps the pages U|W (RWX for M4 — NX
//! arrives with the M5 hardening pass). Every header field is range-checked
//! against the image and kernel-half vaddrs are rejected, so a malformed ELF
//! fails the load instead of panicking or writing kernel page tables.

use fantuan_abi::PHYS_OFFSET;

use crate::mm::frame;
use crate::mm::paging::phys_to_virt;
use crate::mm::user;

const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const PT_LOAD: u32 = 1;
const ELF64_HEADER: usize = 64;
/// 64-bit program header size (phentsize must be at least this).
const PHENT_SIZE: usize = 56;

pub fn load(elf: &[u8]) -> Option<(u64, u64)> {
    if elf.len() < ELF64_HEADER || elf[0..4] != ELF_MAGIC {
        crate::serial::line("elf: bad magic or too small");
        return None;
    }
    if elf[4] != 2 || elf[5] != 1 {
        crate::serial::line("elf: not 64-bit LE");
        return None;
    }
    if u16::from_le_bytes([elf[16], elf[17]]) != 2 {
        crate::serial::line("elf: not ET_EXEC");
        return None;
    }
    if u16::from_le_bytes([elf[18], elf[19]]) != 0x3E {
        crate::serial::line("elf: not x86_64");
        return None;
    }
    let entry = u64::from_le_bytes(elf[24..32].try_into().ok()?);
    let phoff = u64::from_le_bytes(elf[32..40].try_into().ok()?);
    let phentsize = u16::from_le_bytes([elf[54], elf[55]]) as usize;
    let phnum = u16::from_le_bytes([elf[56], elf[57]]) as usize;

    // The program-header table must lie inside the image (checked math: the
    // values come from the file and must not wrap).
    if phentsize < PHENT_SIZE || phnum == 0 {
        crate::serial::line("elf: bad program-header table");
        return None;
    }
    let ph_table_end = phoff.checked_add((phnum as u64).checked_mul(phentsize as u64)?)?;
    if ph_table_end > elf.len() as u64 {
        crate::serial::line("elf: program headers outside the image");
        return None;
    }
    // The entry point is a user address too.
    if entry >= PHYS_OFFSET {
        crate::serial::line("elf: entry in the kernel half");
        return None;
    }

    let cr3 = user::new_user_pml4();

    // Track already-mapped pages: two segments may share a 4K page (e.g.
    // .rodata and .data), and the second must reuse the first's frame — a
    // fresh frame would shadow the earlier content. The entry also carries
    // the accumulated protection so a shared page gets the union of rights
    // (W if any segment writes it; executable if any segment executes it).
    let mut mapped: [(u64, u64, u64); 64] = [(0, 0, 0); 64];
    let mut mapped_n = 0;

    for i in 0..phnum {
        let ph = &elf[phoff as usize + i * phentsize..][..phentsize];
        if u32::from_le_bytes([ph[0], ph[1], ph[2], ph[3]]) != PT_LOAD {
            continue;
        }
        let p_flags = u32::from_le_bytes([ph[4], ph[5], ph[6], ph[7]]);
        let p_offset = u64::from_le_bytes(ph[8..16].try_into().ok()?) as usize;
        let p_vaddr = u64::from_le_bytes(ph[16..24].try_into().ok()?);
        let p_filesz = u64::from_le_bytes(ph[32..40].try_into().ok()?);
        let p_memsz = u64::from_le_bytes(ph[40..48].try_into().ok()?);
        // The segment must fit the user half and its file bytes must exist.
        if p_vaddr.checked_add(p_memsz)? > PHYS_OFFSET {
            crate::serial::line("elf: PT_LOAD overflows the user half");
            return None;
        }
        if p_offset.checked_add(p_filesz as usize)? > elf.len() || p_filesz > p_memsz {
            crate::serial::line("elf: PT_LOAD file bytes outside the image");
            return None;
        }
        // W^X from the program-header flags (PF_X = 1, PF_W = 2): writable
        // only when PF_W, non-executable unless PF_X.
        let prot = user::P_PRESENT
            | user::P_USER
            | if p_flags & 2 != 0 { user::P_WRITABLE } else { 0 }
            | if p_flags & 1 == 0 { user::P_NX } else { 0 };
        let p_filesz = p_filesz as usize;
        let page_start = p_vaddr & !0xFFF;
        let seg_end = p_vaddr + p_memsz;
        let mut page = page_start;
        while page < seg_end {
            let head = if page == page_start { (p_vaddr - page_start) as usize } else { 0 };
            let f = match mapped[..mapped_n].iter().position(|e| e.0 == page) {
                Some(idx) => {
                    // Shared page: union the protections; executable wins.
                    let phys = mapped[idx].1;
                    let exec = mapped[idx].2 & user::P_NX == 0 || prot & user::P_NX == 0;
                    let mut merged = mapped[idx].2 | prot;
                    if exec {
                        merged &= !user::P_NX;
                    }
                    mapped[idx].2 = merged;
                    user::map_page(cr3, page, phys, merged);
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
                    user::map_page(cr3, page, f, prot);
                    let dst = phys_to_virt(f) as *mut u8;
                    unsafe { core::ptr::write_bytes(dst, 0, 4096) };
                    f
                }
            };
            let seg_off = ((page + head as u64) - p_vaddr) as usize; // offset into the segment
            let dst = phys_to_virt(f) as *mut u8;
            if seg_off < p_filesz {
                let copy = (p_filesz - seg_off).min(4096 - head);
                let src = elf.get(p_offset + seg_off..p_offset + seg_off + copy)?;
                unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst.add(head), copy) };
            }
            page += 4096;
        }
    }

    Some((entry, cr3))
}
