//! Minimal static-ELF loader (M4): ET_EXEC, x86_64, PT_LOAD only.
//!
//! Loads each loadable segment into fresh frames, copies the file bytes,
//! zeroes the rest of the segment, and maps the pages U|W (RWX for M4 — NX
//! arrives with the M5 hardening pass). Returns (entry, cr3).

use crate::mm::frame;
use crate::mm::paging::phys_to_virt;
use crate::mm::user;

const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const PT_LOAD: u32 = 1;

pub fn load(elf: &[u8]) -> Option<(u64, u64)> {
    if elf.len() < 64 || elf[0..4] != ELF_MAGIC {
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

    let cr3 = user::new_user_pml4();

    // Track already-mapped pages: two segments may share a 4K page (e.g.
    // .rodata and .data), and the second must reuse the first's frame — a
    // fresh frame would shadow the earlier content.
    let mut mapped: [(u64, u64); 64] = [(0, 0); 64];
    let mut mapped_n = 0;

    for i in 0..phnum {
        let ph = &elf[phoff as usize + i * phentsize..];
        if ph.len() < 56 || u32::from_le_bytes([ph[0], ph[1], ph[2], ph[3]]) != PT_LOAD {
            continue;
        }
        let p_offset = u64::from_le_bytes(ph[8..16].try_into().ok()?) as usize;
        let p_vaddr = u64::from_le_bytes(ph[16..24].try_into().ok()?);
        let p_filesz = u64::from_le_bytes(ph[32..40].try_into().ok()?) as usize;
        let p_memsz = u64::from_le_bytes(ph[40..48].try_into().ok()?) as usize;
        let page_start = p_vaddr & !0xFFF;
        let seg_end = p_vaddr + p_memsz as u64;
        let mut page = page_start;
        while page < seg_end {
            let head = if page == page_start { (p_vaddr - page_start) as usize } else { 0 };
            let f = match mapped[..mapped_n].iter().find(|(v, _)| *v == page) {
                Some(&(_, phys)) => phys, // shared page: overlay this segment
                None => {
                    let f = frame::get().alloc()?;
                    if mapped_n >= mapped.len() {
                        // More distinct pages than the tracking table holds:
                        // refuse rather than map an untracked page (which a
                        // later segment could double-allocate).
                        return None;
                    }
                    mapped[mapped_n] = (page, f);
                    mapped_n += 1;
                    user::map_page(cr3, page, f, user::P_PRESENT | user::P_WRITABLE | user::P_USER);
                    let dst = phys_to_virt(f) as *mut u8;
                    unsafe { core::ptr::write_bytes(dst, 0, 4096) };
                    f
                }
            };
            let seg_off = (page + head as u64) - p_vaddr; // offset into the segment
            let dst = phys_to_virt(f) as *mut u8;
            if (seg_off as usize) < p_filesz {
                let copy = (p_filesz - seg_off as usize).min(4096 - head);
                let src = &elf[p_offset + seg_off as usize..p_offset + seg_off as usize + copy];
                unsafe { core::ptr::copy_nonoverlapping(src.as_ptr(), dst.add(head), copy) };
            }
            page += 4096;
        }
    }

    Some((entry, cr3))
}
