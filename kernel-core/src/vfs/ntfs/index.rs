//! NTFS $I30 directory enumeration (M12-4): the resident INDEX_ROOT plus
//! the INDEX_ALLOCATION B-tree, walked breadth-first with a bounded queue
//! and one caller-supplied scratch block. Read-only; corrupt trees are
//! rejected instead of followed.

use super::record::{le16, le32, le64, AttrValue, Record, ATTR_INDEX_ALLOC, ATTR_INDEX_ROOT};
use super::{Ntfs, NtfsErr};

const INDEX_TYPE_FILE_NAME: u32 = 0x30;
const COLLATION_FILE_NAME: u32 = 0x01;
const ENTRY_HAS_SUBNODE: u16 = 0x01;
const ENTRY_LAST: u16 = 0x02;
/// Index tree bounds: a rescue listing never needs more, and a cyclic or
/// crafted tree stops here instead of spinning.
const MAX_INDEX_BLOCKS: usize = 64;

impl Ntfs {
    /// Resolve a '/'-separated path relative to the volume root.
    pub fn resolve(&self, path: &[u8], scratch: &mut [u8]) -> Result<Record, NtfsErr> {
        let mut rec = self.read_record(5)?;
        for comp in path.split(|&b| b == b'/') {
            if comp.is_empty() {
                continue;
            }
            if !rec.is_dir() {
                return Err(NtfsErr::NotDir);
            }
            let mut found = None;
            self.walk_dir(&rec, scratch, &mut |name, child, _dir, _size| {
                if found.is_none() && super::name_eq(name, comp) {
                    found = Some(child);
                }
                found.is_none()
            })?;
            rec = self.read_record(found.ok_or(NtfsErr::NotFound)?)?;
        }
        Ok(rec)
    }

    /// Enumerate a directory; F is called per visible entry with
    /// (name, MFT record, is_dir, data size) and returns false to stop.
    pub fn list(
        &self,
        path: &[u8],
        scratch: &mut [u8],
        f: &mut impl FnMut(&[u8], u64, bool, u64) -> bool,
    ) -> Result<(), NtfsErr> {
        let rec = self.resolve(path, scratch)?;
        if !rec.is_dir() {
            return Err(NtfsErr::NotDir);
        }
        self.walk_dir(&rec, scratch, f)
    }

    /// The INDEX-th visible entry of a directory record, copied into NAME.
    pub fn entry_at(
        &self,
        rec: &Record,
        index: u64,
        scratch: &mut [u8],
        name: &mut [u8],
    ) -> Result<Option<(u64, bool, u64, usize)>, NtfsErr> {
        let mut seen = 0u64;
        let mut out = None;
        self.walk_dir(rec, scratch, &mut |n, child, is_dir, size| {
            if seen == index {
                let len = n.len().min(name.len());
                name[..len].copy_from_slice(&n[..len]);
                out = Some((child, is_dir, size, len));
                return false;
            }
            seen += 1;
            true
        })?;
        Ok(out)
    }

    /// Walk a directory record's visible entries in index order.
    pub fn walk_dir(
        &self,
        rec: &Record,
        scratch: &mut [u8],
        f: &mut impl FnMut(&[u8], u64, bool, u64) -> bool,
    ) -> Result<(), NtfsErr> {
        let value = match rec.find(ATTR_INDEX_ROOT, true)? {
            Some(v) => v,
            None => rec.find(ATTR_INDEX_ROOT, false)?.ok_or(NtfsErr::Record)?,
        };
        let AttrValue::Resident(v) = value else { return Err(NtfsErr::Record) };
        if v.len() < 0x20 || le32(v, 0) != INDEX_TYPE_FILE_NAME || le32(v, 4) != COLLATION_FILE_NAME {
            return Err(NtfsErr::Record);
        }
        let start = 0x10usize.checked_add(le32(v, 0x10) as usize).ok_or(NtfsErr::Record)?;
        let end = 0x10usize.checked_add(le32(v, 0x14) as usize).ok_or(NtfsErr::Record)?;
        if start < 0x20 || end > v.len() || end < start {
            return Err(NtfsErr::Record);
        }
        let mut queue = [0u64; MAX_INDEX_BLOCKS];
        let mut qlen = 0usize;
        self.walk_node(v, start, end, &mut queue, &mut qlen, f)?;
        let bs = self.index_block_size as usize;
        let mut qi = 0usize;
        while qi < qlen {
            let vcn = queue[qi];
            qi += 1;
            self.read_index_block(rec, vcn, scratch)?;
            let start = 0x18usize.checked_add(le32(scratch, 0x18) as usize).ok_or(NtfsErr::Record)?;
            let end = 0x18usize.checked_add(le32(scratch, 0x1C) as usize).ok_or(NtfsErr::Record)?;
            if start < 0x18 || end > bs || end < start {
                return Err(NtfsErr::Record);
            }
            self.walk_node(&scratch[..bs], start, end, &mut queue, &mut qlen, f)?;
        }
        Ok(())
    }

    /// Parse the entries between START and END, emit the keys and enqueue
    /// the child VCNs (breadth-first, bounded).
    fn walk_node(
        &self,
        data: &[u8],
        start: usize,
        end: usize,
        queue: &mut [u64; MAX_INDEX_BLOCKS],
        qlen: &mut usize,
        f: &mut impl FnMut(&[u8], u64, bool, u64) -> bool,
    ) -> Result<(), NtfsErr> {
        let mut p = start;
        while p + 0x10 <= end && p + 0x10 <= data.len() {
            let file_ref = le64(data, p) & 0x0000_FFFF_FFFF_FFFF;
            let elen = le16(data, p + 8) as usize;
            let klen = le16(data, p + 10) as usize;
            let flags = le16(data, p + 12);
            if elen < 0x10 || elen % 8 != 0 || p + elen > end || p + elen > data.len() {
                return Err(NtfsErr::Record);
            }
            if flags & ENTRY_HAS_SUBNODE != 0 {
                if elen < 0x18 || *qlen == queue.len() {
                    return Err(NtfsErr::Corrupt);
                }
                queue[*qlen] = le64(data, p + elen - 8);
                *qlen += 1;
            }
            if flags & ENTRY_LAST == 0 {
                if p + 0x10 + klen > data.len() {
                    return Err(NtfsErr::Record);
                }
                let key = &data[p + 0x10..p + 0x10 + klen];
                if key.len() < 0x42 {
                    return Err(NtfsErr::Record);
                }
                let name_len = key[0x40] as usize;
                let namespace = key[0x41];
                if 0x42 + name_len * 2 > key.len() {
                    return Err(NtfsErr::Record);
                }
                let mut name = [0u8; 256];
                if let Some(n) = super::utf16_to_utf8(&key[0x42..0x42 + name_len * 2], &mut name) {
                    let attrs = le32(key, 0x38);
                    let size = le64(key, 0x30);
                    if super::visible(&name[..n], namespace)
                        && !f(&name[..n], file_ref, attrs & 0x1000_0000 != 0, size)
                    {
                        return Ok(());
                    }
                }
            }
            p += elen;
            if flags & ENTRY_LAST != 0 {
                break;
            }
        }
        Ok(())
    }

    fn read_index_block(&self, rec: &Record, vcn: u64, scratch: &mut [u8]) -> Result<(), NtfsErr> {
        let bs = self.index_block_size as usize;
        if scratch.len() < bs {
            return Err(NtfsErr::Corrupt);
        }
        let Some(AttrValue::NonResident(nr)) = rec.find(ATTR_INDEX_ALLOC, true)? else {
            return Err(NtfsErr::Corrupt);
        };
        let off = vcn.checked_mul(bs as u64).ok_or(NtfsErr::Corrupt)?;
        if self.read_runs_at(&nr.runs, off, &mut scratch[..bs])? != bs {
            return Err(NtfsErr::Io);
        }
        if &scratch[..4] != b"INDX" {
            return Err(NtfsErr::Record);
        }
        let usa_off = le16(scratch, 4) as usize;
        let usa_count = le16(scratch, 6) as usize;
        if !super::record::apply_fixups(&mut scratch[..bs], usa_off, usa_count) {
            return Err(NtfsErr::Fixup);
        }
        Ok(())
    }
}
