//! NTFS FILE record and attribute parsing (M12-4): update-sequence fixups,
//! the attribute iterator, resident values and non-resident runlists.
//!
//! Every read is bounds-checked: a crafted or corrupt record can only make
//! a parse fail, never panic. Compression and encryption are rejected here;
//! attribute lists are detected and reported as a v2 feature.

use super::runlist::RunList;
use super::NtfsErr;

pub const MAX_RECORD: usize = 1024;

pub const ATTR_STANDARD_INFO: u32 = 0x10;
pub const ATTR_ATTRIBUTE_LIST: u32 = 0x20;
pub const ATTR_VOLUME_NAME: u32 = 0x60;
pub const ATTR_DATA: u32 = 0x80;
pub const ATTR_INDEX_ROOT: u32 = 0x90;
pub const ATTR_INDEX_ALLOC: u32 = 0xA0;

/// Attribute-header flags.
const ATTR_FLAG_COMPRESSED: u16 = 0x0001;
const ATTR_FLAG_ENCRYPTED: u16 = 0x4000;

pub fn le16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

pub fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

pub fn le64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3], b[o + 4], b[o + 5], b[o + 6], b[o + 7]])
}

/// A decoded non-resident attribute.
#[derive(Clone, Copy)]
pub struct NonResident {
    pub alloc_size: u64,
    pub data_size: u64,
    pub init_size: u64,
    pub runs: RunList,
}

/// An attribute value as far as the read path needs it.
pub enum AttrValue<'a> {
    Resident(&'a [u8]),
    NonResident(NonResident),
}

/// One FILE record with its fixups applied.
pub struct Record {
    pub buf: [u8; MAX_RECORD],
    pub number: u64,
    pub flags: u16,
    pub attrs_off: usize,
}

impl Record {
    /// Copy REC_SIZE bytes and undo the update-sequence protection.
    pub fn parse(number: u64, raw: &[u8], rec_size: usize) -> Result<Record, NtfsErr> {
        if rec_size == 0 || rec_size > MAX_RECORD || raw.len() < rec_size || rec_size % 512 != 0 {
            return Err(NtfsErr::Record);
        }
        if &raw[0..4] != b"FILE" {
            return Err(NtfsErr::Record);
        }
        let mut buf = [0u8; MAX_RECORD];
        buf[..rec_size].copy_from_slice(&raw[..rec_size]);
        let usa_off = le16(&buf, 4) as usize;
        let usa_count = le16(&buf, 6) as usize;
        if !apply_fixups(&mut buf[..rec_size], usa_off, usa_count) {
            return Err(NtfsErr::Fixup);
        }
        let flags = le16(&buf, 0x16);
        Ok(Record { buf, number, flags, attrs_off: le16(&buf, 0x14) as usize })
    }

    /// In-use flag (0x01); a free record is still parsed for its fixups.
    pub fn is_used(&self) -> bool {
        self.flags & 0x01 != 0
    }

    pub fn is_dir(&self) -> bool {
        self.flags & 0x02 != 0
    }

    /// Find the first attribute of TYPE; NAMED selects a non-empty name.
    pub fn find(&self, ty: u32, named: bool) -> Result<Option<AttrValue<'_>>, NtfsErr> {
        let mut off = self.attrs_off;
        if off < 0x18 || off + 8 > MAX_RECORD {
            return Err(NtfsErr::Record);
        }
        while off + 8 <= MAX_RECORD {
            let t = le32(&self.buf, off);
            if t == 0xFFFF_FFFF {
                return Ok(None);
            }
            let len = le32(&self.buf, off + 4) as usize;
            if len < 0x18 || off + len > MAX_RECORD {
                return Err(NtfsErr::Record);
            }
            let non_resident = self.buf[off + 8] != 0;
            let name_len = self.buf[off + 9] as usize;
            if t == ty && (name_len != 0) == named {
                return Ok(Some(self.attr_value(off, len, non_resident)?));
            }
            off += len;
        }
        Err(NtfsErr::Record)
    }

    fn attr_value(&self, off: usize, len: usize, non_resident: bool) -> Result<AttrValue<'_>, NtfsErr> {
        if !non_resident {
            let vlen = le32(&self.buf, off + 0x10) as usize;
            let voff = le16(&self.buf, off + 0x14) as usize;
            if voff + vlen > len {
                return Err(NtfsErr::Record);
            }
            return Ok(AttrValue::Resident(&self.buf[off + voff..off + voff + vlen]));
        }
        let flags = le16(&self.buf, off + 0x0C);
        if flags & (ATTR_FLAG_COMPRESSED | ATTR_FLAG_ENCRYPTED) != 0 {
            return Err(NtfsErr::Unsupported);
        }
        if le64(&self.buf, off + 0x10) != 0 {
            return Err(NtfsErr::AttrList); // runlist continuation record
        }
        let run_off = le16(&self.buf, off + 0x20) as usize;
        if run_off < 0x40 || off + run_off > off + len {
            return Err(NtfsErr::RunList);
        }
        let runs = RunList::decode(&self.buf[off + run_off..off + len])?;
        Ok(AttrValue::NonResident(NonResident {
            alloc_size: le64(&self.buf, off + 0x28),
            data_size: le64(&self.buf, off + 0x30),
            init_size: le64(&self.buf, off + 0x38),
            runs,
        }))
    }

    /// The unnamed $DATA value, when present.
    pub fn data(&self) -> Result<Option<AttrValue<'_>>, NtfsErr> {
        match self.find(ATTR_DATA, false) {
            Ok(v) => Ok(v),
            Err(NtfsErr::AttrList) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Object size in bytes: the $DATA length, 0 for directories.
    pub fn size(&self) -> u64 {
        match self.data() {
            Ok(Some(AttrValue::Resident(v))) => v.len() as u64,
            Ok(Some(AttrValue::NonResident(nr))) => nr.data_size,
            _ => 0,
        }
    }

    /// The $STANDARD_INFORMATION file-attribute word (0 when absent). Bit
    /// 0x01 is read-only; 0x10000000 marks a directory.
    pub fn file_attributes(&self) -> u32 {
        match self.find(ATTR_STANDARD_INFO, false) {
            Ok(Some(AttrValue::Resident(v))) if v.len() >= 0x24 => le32(v, 0x20),
            _ => 0,
        }
    }

    /// Whether the record carries an attribute list (a v2 feature).
    pub fn has_attr_list(&self) -> bool {
        matches!(self.find(ATTR_ATTRIBUTE_LIST, false), Ok(Some(_)) | Err(NtfsErr::AttrList))
    }
}

/// Undo the update-sequence protection in place. False on a mismatch:
/// every sector must end with the record's USN. Shared with the INDX
/// index-allocation blocks (same protection, different magic).
pub fn apply_fixups(buf: &mut [u8], usa_off: usize, usa_count: usize) -> bool {
    let sectors = buf.len() / 512;
    if buf.len() == 0 || buf.len() % 512 != 0 || usa_count != sectors + 1 {
        return false;
    }
    if usa_off < 8 || usa_off + usa_count * 2 > buf.len() {
        return false;
    }
    let usn = [buf[usa_off], buf[usa_off + 1]];
    for i in 1..usa_count {
        let end = i * 512 - 2;
        if buf[end] != usn[0] || buf[end + 1] != usn[1] {
            return false;
        }
        buf[end] = buf[usa_off + 2 * i];
        buf[end + 1] = buf[usa_off + 2 * i + 1];
    }
    true
}
