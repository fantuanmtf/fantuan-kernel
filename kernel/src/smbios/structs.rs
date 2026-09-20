//! SMBIOS structure-table walk (types 0/1/4/9/17) — split out of mod.rs to
//! keep every file inside the size rule. The parser owns no state of its
//! own; it fills the catalog the parent module exposes.

use super::{
    get_nth_string, phys_to_virt, strref_to_str, MemoryDevice, BIOS, CORRUPT,
    DIMMS, DIMM_COUNT, MAX_DIMMS, SYSTEM, TABLE_BUF, TABLE_BUF_LEN,
};
#[cfg(kconfig_graphics)]
use super::{SlotInfo, MAX_SLOTS, SLOTS, SLOT_COUNT};

// --- Structure parsers: Type 0/1/4/9/17 ---

pub(super) unsafe fn parse_structs(table_phys: u64, table_len: usize) {
    let buf = core::ptr::addr_of_mut!(TABLE_BUF).cast::<u8>();
    let len = table_len.min(TABLE_BUF_LEN);
    let buf_end = buf.add(len);
    let src = phys_to_virt(table_phys) as *const u8;
    for i in 0..len {
        *buf.add(i) = *src.add(i);
    }

    let mut pos = buf;
    let tlim = buf_end;
    while pos.add(4) <= tlim {
        let stype = *pos;
        let slen = *pos.add(1) as usize;
        if slen < 4 {
            CORRUPT = true;
            break;
        }
        let hdr_end = pos.add(slen);
        if hdr_end > tlim {
            CORRUPT = true;
            break;
        }

        let mut str_start = hdr_end;
        while str_start.add(1) < tlim && !(*str_start == 0 && *str_start.add(1) == 0) {
            str_start = str_start.add(1);
        }
        if str_start.add(1) >= tlim {
            CORRUPT = true;
            break;
        }
        let str_area = hdr_end;
        let str_end = str_start.add(1);

        match stype {
            0 => {
                if slen >= 0x12 {
                    let v = get_nth_string(str_area, str_end, *pos.add(0x4));
                    let ver = get_nth_string(str_area, str_end, *pos.add(0x5));
                    let rel = get_nth_string(str_area, str_end, *pos.add(0x8));
                    BIOS = Some((v, ver, rel));
                }
            }
            1 => {
                if slen >= 0x08 {
                    let mfr = get_nth_string(str_area, str_end, *pos.add(0x4));
                    let prod = get_nth_string(str_area, str_end, *pos.add(0x5));
                    let ser = get_nth_string(str_area, str_end, *pos.add(0x7));
                    SYSTEM = Some((mfr, prod, ser));
                }
            }
            4 => {
                if slen >= 0x11 {
                    let _ = get_nth_string(str_area, str_end, *pos.add(0x7));
                    let _ = get_nth_string(str_area, str_end, *pos.add(0x10));
                }
            }
            #[cfg(kconfig_graphics)]
            9 => {
                if slen >= 0x0B && SLOT_COUNT < MAX_SLOTS {
                    let desg = get_nth_string(str_area, str_end, *pos.add(0x4));
                    let slot_type = *pos.add(0x5);
                    let usage = *pos.add(0x0A);
                    // SMBIOS 3.x slot types: 0x06 PCI, 0x08-0x25 the PCI
                    // Express family (x1..x16 across generations), 0x30-0x33
                    // AGP. A discrete GPU can sit in any of them; M.2 and
                    // other special-purpose slots (0xB5+) are excluded.
                    let uses_pci = slot_type == 0x06
                        || (0x08..=0x25).contains(&slot_type)
                        || (0x30..=0x33).contains(&slot_type);
                    let display = uses_pci;
                    SLOTS[SLOT_COUNT] = SlotInfo {
                        slot_id: SLOT_COUNT as u8,
                        designation: "",
                        in_use: usage == 0x01,
                        uses_pci,
                        display_class_hint: display,
                    };
                    let sr = strref_to_str(desg);
                    SLOTS[SLOT_COUNT].designation = sr;
                    SLOT_COUNT += 1;
                }
            }
            17 => {
                if slen >= 0x1B && DIMM_COUNT < MAX_DIMMS {
                    // Size field: bit15 clear => MiB; bit15 set => KiB;
                    // 0 => not installed; 0x7FFF => extended size (MiB) at 0x1C;
                    // 0xFFFF => unknown.
                    // Type 17: size at 0x0C (2), speed at 0x15 (2),
                    // manufacturer string 0x17, part number string 0x1A.
                    let size16 = *(pos.add(0x0C) as *const u16);
                    let size_mb: u32 = if size16 == 0 || size16 == 0xFFFF {
                        0
                    } else if size16 == 0x7FFF {
                        if slen >= 0x20 {
                            *(pos.add(0x1C) as *const u32) & 0x7FFF_FFFF
                        } else {
                            0
                        }
                    } else if size16 & 0x8000 != 0 {
                        ((size16 & 0x7FFF) as u32 + 1023) / 1024
                    } else {
                        size16 as u32
                    };
                    let speed = *(pos.add(0x15) as *const u16);
                    let mfr = get_nth_string(str_area, str_end, *pos.add(0x17));
                    let part = get_nth_string(str_area, str_end, *pos.add(0x1A));
                    let ms = strref_to_str(mfr);
                    let ps = strref_to_str(part);
                    DIMMS[DIMM_COUNT] = MemoryDevice { size_mb, speed_mtps: speed, manufacturer: ms, part_number: ps };
                    DIMM_COUNT += 1;
                }
            }
            _ => {}
        }
        pos = str_end.add(1);
    }
}
