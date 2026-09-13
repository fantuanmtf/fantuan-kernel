//! SMBIOS parsing (M5.5, DESIGN.md §3 §6.2): entry-point scan, checksum
//! validation, string-table sanitization, and structure-parsers for the
//! diagnostic-relevant types (0/1/4/9/16/17). Single-threaded boot-time only;
//! uses static mut buffers with a one-shot init flag.

use fantuan_abi::PHYS_OFFSET;

const fn phys_to_virt(phys: u64) -> u64 { PHYS_OFFSET + phys }

// --- Entry-point scanning & checksum validation ---

const TABLE_BUF_LEN: usize = 8192;
static mut INITED: bool = false;
/// Set when the structure walk had to abort early (bad length / out of
/// bounds) — reported as "smbios: abort (corrupt)" by the diagnostics.
static mut CORRUPT: bool = false;
static mut TABLE_BUF: [u8; TABLE_BUF_LEN] = [0; TABLE_BUF_LEN];
const POOL_LEN: usize = 8192;
static mut STR_POOL: [u8; POOL_LEN] = [0; POOL_LEN];
static mut STR_LEN: usize = 0;

#[derive(Clone, Copy)]
struct StrRef { off: usize, len: usize }

static mut BIOS: Option<(StrRef, StrRef, StrRef)> = None;
static mut SYSTEM: Option<(StrRef, StrRef, StrRef)> = None;
const MAX_SLOTS: usize = 64;
static mut SLOTS: [SlotInfo; MAX_SLOTS] = [SlotInfo::ZERO; MAX_SLOTS];
static mut SLOT_COUNT: usize = 0;
const MAX_DIMMS: usize = 32;
static mut DIMMS: [MemoryDevice; MAX_DIMMS] = [MemoryDevice::ZERO; MAX_DIMMS];
static mut DIMM_COUNT: usize = 0;

/// Copy one NUL-terminated SMBIOS string into the pool, sanitized to
/// printable ASCII. Trailing spaces are trimmed *before* copying so earlier
/// strings in the pool are never touched. Bounded by MAX (256 bytes).
unsafe fn store_str(src: *const u8, max: usize) -> StrRef {
    // addr_of_mut! keeps this a raw-pointer access — no reference to the
    // static is ever formed (single-threaded boot-time code).
    let pool = core::ptr::addr_of_mut!(STR_POOL).cast::<u8>();
    let mut n = 0;
    while n < max && n < 256 && *src.add(n) != 0 {
        n += 1;
    }
    while n > 0 && *src.add(n - 1) == b' ' {
        n -= 1;
    }
    if n == 0 || STR_LEN + n > POOL_LEN {
        return StrRef { off: 0, len: 0 };
    }
    let off = STR_LEN;
    for k in 0..n {
        let b = *src.add(k);
        *pool.add(off + k) = if b < 0x20 || b == 0x7F || b >= 0x80 { b'?' } else { b };
    }
    STR_LEN += n;
    StrRef { off, len: n }
}

fn strref_to_str(r: StrRef) -> &'static str {
    unsafe {
        let base = core::ptr::addr_of!(STR_POOL).cast::<u8>();
        let slice = core::slice::from_raw_parts(base.add(r.off), r.len);
        core::str::from_utf8(slice).unwrap_or("?")
    }
}

fn checksum_ok(base: *const u8, len: usize) -> bool {
    let mut sum: u8 = 0;
    unsafe { for i in 0..len { sum = sum.wrapping_add(*base.add(i)); } }
    sum == 0
}

unsafe fn find_entry(start: u64, len: u64) -> Option<(u64, usize, bool)> {
    let mut p = start;
    while p + 4 <= start + len {
        let v = phys_to_virt(p) as *const u32;
        let sig = *v;
        if sig == 0x5F4D535F { // "_SM_" (32-bit entry point)
            let ep = phys_to_virt(p) as *const u8;
            let ep_len = *ep.add(5) as usize;
            if ep_len >= 0x1F && checksum_ok(ep, ep_len) {
                let table_addr = *(ep.add(0x18) as *const u32) as u64;
                let table_len = *(ep.add(0x16) as *const u16) as usize;
                return Some((table_addr, table_len, false));
            }
        }
        if sig == 0x334D535F { // "_SM3_" (64-bit entry point)
            let ep = phys_to_virt(p) as *const u8;
            let ep_len = *(ep.add(6) as *const u8) as usize;
            if ep_len >= 0x18 && checksum_ok(ep, ep_len) {
                let table_addr = *(ep.add(0x10) as *const u64);
                let max_struct = *(ep.add(0x0C) as *const u32) as usize;
                return Some((table_addr, max_struct, true));
            }
        }
        p += 16;
    }
    None
}

// --- String-table helpers (printable-ASCII sanitize) ---

unsafe fn get_nth_string(str_area: *const u8, area_end: *const u8, n: u8) -> StrRef {
    if n == 0 { return StrRef { off: 0, len: 0 }; }
    let mut cur = str_area;
    let mut count: u8 = 0;
    while cur < area_end {
        let mut end = cur;
        while end < area_end && *end != 0 { end = end.add(1); }
        if end == cur { break; }
        count += 1;
        if count == n { return store_str(cur, (end as usize) - (cur as usize)); }
        cur = end.add(1);
    }
    StrRef { off: 0, len: 0 }
}

// --- Structure parsers: Type 0/1/4/9/16/17 ---

unsafe fn parse_structs(table_phys: u64, table_len: usize) {
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
            9 => {
                if slen >= 0x0B && SLOT_COUNT < MAX_SLOTS {
                    let desg = get_nth_string(str_area, str_end, *pos.add(0x4));
                    let slot_type = *pos.add(0x5);
                    let usage = *pos.add(0x0A);
                    let uses_pci = slot_type >= 0x04 && slot_type <= 0x07;
                    let display = slot_type >= 0x08 && slot_type <= 0x0C;
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

// --- Query API exports + static catalog ---

unsafe fn reset_catalog() {
    INITED = true;
    CORRUPT = false;
    STR_LEN = 0;
    SLOT_COUNT = 0;
    DIMM_COUNT = 0;
    BIOS = None;
    SYSTEM = None;
}

/// True when the last parse aborted on a malformed structure.
pub fn corrupt() -> bool {
    unsafe { CORRUPT }
}

/// Fallback discovery: scan a physical region (the F-segment on legacy
/// firmware) for a 16-bit/64-bit SMBIOS anchor.
pub unsafe fn init(phys_start_scan_region: u64, len: u64) {
    if INITED { return; }
    reset_catalog();
    if let Some((taddr, tlen, _)) = find_entry(phys_start_scan_region, len) {
        parse_structs(taddr, tlen);
    }
}

/// Primary discovery (M5.5): the SMBIOS entry point the firmware published in
/// its UEFI configuration table (BootInfo.smbios_table). Validates the anchor
/// and checksum before trusting the table address.
pub unsafe fn init_from_entry(entry_phys: u64) {
    if INITED || entry_phys == 0 {
        return;
    }
    let ep = phys_to_virt(entry_phys) as *const u8;
    let sig = *(ep as *const u32);
    if sig == 0x334D535F {
        // SMBIOS 3.0: entry point length at +6, table address at +0x10.
        let ep_len = *ep.add(6) as usize;
        if ep_len >= 0x18 && checksum_ok(ep, ep_len) {
            let table_addr = *(ep.add(0x10) as *const u64);
            let max_struct = *(ep.add(0x0C) as *const u32) as usize;
            reset_catalog();
            parse_structs(table_addr, max_struct);
        }
    } else if sig == 0x5F4D535F {
        // SMBIOS 2.x: length at +5, table length at +0x16, address at +0x18.
        let ep_len = *ep.add(5) as usize;
        if ep_len >= 0x1F && checksum_ok(ep, ep_len) {
            let table_addr = *(ep.add(0x18) as *const u32) as u64;
            let table_len = *(ep.add(0x16) as *const u16) as usize;
            reset_catalog();
            parse_structs(table_addr, table_len);
        }
    }
}

pub fn bios_info() -> Option<BiosInfo> {
    unsafe {
        BIOS.map(|(v, ver, rel)| BiosInfo {
            vendor: strref_to_str(v),
            version: strref_to_str(ver),
            release_date: strref_to_str(rel),
        })
    }
}

pub fn system_info() -> Option<SystemInfo> {
    unsafe {
        SYSTEM.map(|(m, p, s)| SystemInfo {
            manufacturer: strref_to_str(m),
            product_name: strref_to_str(p),
            serial_number: strref_to_str(s),
        })
    }
}

pub fn system_slots() -> &'static [SlotInfo] {
    unsafe { &SLOTS[..SLOT_COUNT] }
}

pub fn memory_devices() -> &'static [MemoryDevice] {
    unsafe { &DIMMS[..DIMM_COUNT] }
}

pub struct BiosInfo { pub vendor: &'static str, pub version: &'static str, pub release_date: &'static str }
pub struct SystemInfo { pub manufacturer: &'static str, pub product_name: &'static str, pub serial_number: &'static str }
pub struct SlotInfo { pub slot_id: u8, pub designation: &'static str, pub in_use: bool, pub uses_pci: bool, pub display_class_hint: bool }
impl SlotInfo { const ZERO: SlotInfo = SlotInfo { slot_id: 0, designation: "", in_use: false, uses_pci: false, display_class_hint: false }; }
pub struct MemoryDevice { pub size_mb: u32, pub speed_mtps: u16, pub manufacturer: &'static str, pub part_number: &'static str }
impl MemoryDevice { const ZERO: MemoryDevice = MemoryDevice { size_mb: 0, speed_mtps: 0, manufacturer: "", part_number: "" }; }
