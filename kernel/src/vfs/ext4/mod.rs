//! ext4 read-only driver (M6.5, DESIGN.md §8/§9): superblock + group
//! descriptors, inodes with extent trees, linear directory scan and file
//! reads. Read-only by construction — no journal replay, no writes, no
//! allocation; a filesystem that needs recovery is reported as such.
//!
//! Scope: the fields a rescue system needs — UUID, label, block size, inode
//! and block counts, path lookup, directory listing, file read. Extent trees
//! are walked to depth 0/1; deeper trees (huge fragmented files) and inline
//! data are out of scope for v1.

use core::ffi::c_void;
use core::fmt::Write;

use crate::serial::{self, Serial};

mod extents;

extern "C" {
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
}

const EXT4_MAGIC: u16 = 0xEF53;
const S_IFDIR: u16 = 0x4000;
const S_IFREG: u16 = 0x8000;
pub const EXT4_EXTENTS_FL: u32 = 0x0008_0000;

/// One inode as far as the rescue kernel cares.
#[derive(Clone, Copy)]
pub struct Inode {
    pub num: u32,
    pub mode: u16,
    pub size: u64,
    pub flags: u32,
    pub blocks: [u8; 60],
}

impl Inode {
    pub fn is_dir(&self) -> bool {
        self.mode & 0xF000 == S_IFDIR
    }
    pub fn is_file(&self) -> bool {
        self.mode & 0xF000 == S_IFREG
    }
    pub fn extents(&self) -> bool {
        self.flags & EXT4_EXTENTS_FL != 0
    }
}

#[derive(Clone, Copy)]
pub struct Ext4 {
    pub part_lba: u64,
    pub block_size: u32,
    pub blocks_count: u64,
    pub inodes_count: u32,
    pub inodes_per_group: u32,
    pub inode_size: u32,
    pub inode_table_block: u64,
    pub uuid: [u8; 16],
    pub label: [u8; 16],
    pub needs_recovery: bool,
}

fn le16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Read BLOCKS consecutive blocks of the filesystem into BUF.
fn read_blocks(&self, block: u64, count: u32, buf: &mut [u8]) -> bool {
    let lba = self.part_lba + block * (self.block_size as u64 / 512);
    let sectors = (count * (self.block_size / 512)) as usize;
    if buf.len() < sectors * 512 {
        return false;
    }
    unsafe { blk_read(core::ptr::null_mut(), lba, buf.as_mut_ptr() as *mut c_void, sectors) == 0 }
}

fn read_block(&self, block: u64, buf: &mut [u8]) -> bool {
    self.read_blocks(block, 1, buf)
}

/// Parse the superblock at the partition start (1024 bytes in).
pub fn parse(part_lba: u64) -> Option<Ext4> {
    let mut sb = [0u8; 1024];
    let lba = part_lba + 2; // offset 1024 = sector 2
    if unsafe { blk_read(core::ptr::null_mut(), lba, sb.as_mut_ptr() as *mut c_void, 2) } != 0 {
        return None;
    }
    if le16(&sb, 0x38) != EXT4_MAGIC {
        return None;
    }
    let log_block = le32(&sb, 0x18);
    if log_block > 6 {
        return None;
    }
    let block_size = 1024u32 << log_block;
    let inode_size = if le32(&sb, 0x4C) == 0 { 128 } else { le16(&sb, 0x58) as u32 };
    if inode_size < 128 || inode_size > 1024 {
        return None;
    }
    let incompat = le32(&sb, 0x60);

    // Group descriptor 0 sits in the block after the superblock (block 1 for
    // 1 KiB blocks, where the superblock occupies block 1 too; otherwise
    // block 1 of a >1 KiB filesystem is already past the 1024-byte offset).
    let first_data = if block_size == 1024 { 1u64 } else { 0 };
    let gdt_block = first_data + 1;
    let mut gdt = [0u8; 1024];
    // Read via the raw device: the superblock accessor reads full blocks.
    let fs = Ext4 {
        part_lba,
        block_size,
        blocks_count: le32(&sb, 0x04) as u64,
        inodes_count: le32(&sb, 0x00),
        inodes_per_group: le32(&sb, 0x28),
        inode_size,
        inode_table_block: 0,
        uuid: [0; 16],
        label: [0; 16],
        needs_recovery: incompat & 0x0004 != 0,
    };
    if !fs.read_block(gdt_block, &mut gdt) {
        return None;
    }

    let mut uuid = [0u8; 16];
    uuid.copy_from_slice(&sb[0x68..0x78]);
    let mut label = [0u8; 16];
    label.copy_from_slice(&sb[0x78..0x88]);

    let fs = Ext4 {
        inode_table_block: le32(&gdt, 0x08) as u64,
        uuid,
        label,
        ..fs
    };
    Some(fs)
}

impl Ext4 {
    /// Read inode NUM (1-based) from the group's inode table.
    pub fn read_inode(&self, num: u32) -> Option<Inode> {
        if num == 0 || num > self.inodes_count {
            return None;
        }
        let per_group = self.inodes_per_group.max(1);
        let group = (num - 1) / per_group;
        let index = (num - 1) % per_group;

        // Only group 0's table is cached; other groups re-read the GDT entry.
        let table_block = if group == 0 {
            self.inode_table_block
        } else {
            let first_data = if self.block_size == 1024 { 1u64 } else { 0 };
            let mut gdt = [0u8; 1024];
            let desc_size = 32u64; // 64-byte descriptors need the 64bit feature
            let byte = group as u64 * desc_size;
            let block = first_data + 1 + byte / self.block_size as u64;
            if !self.read_block(block, &mut gdt) {
                return None;
            }
            le32(&gdt, (byte % self.block_size as u64) as usize + 0x08) as u64
        };

        let byte_off = index as u64 * self.inode_size as u64;
        let block = table_block + byte_off / self.block_size as u64;
        let mut buf = [0u8; 4096];
        if !self.read_block(block, &mut buf) {
            return None;
        }
        let o = (byte_off % self.block_size as u64) as usize;
        if o + 128 > buf.len() {
            return None;
        }
        let mut blocks = [0u8; 60];
        blocks.copy_from_slice(&buf[o + 40..o + 100]);
        let size_lo = le32(&buf, o + 4) as u64;
        let size_high = if self.inode_size >= 128 + 8 { le32(&buf, o + 108) as u64 } else { 0 };
        Some(Inode {
            num,
            mode: le16(&buf, o),
            size: size_lo | (size_high << 32),
            flags: le32(&buf, o + 32),
            blocks,
        })
    }

    pub fn root(&self) -> Option<Inode> {
        self.read_inode(2)
    }

    /// Look up a '/' separated path from the root.
    pub fn lookup(&self, path: &[u8]) -> Option<Inode> {
        let mut cur = self.root()?;
        for comp in path.split(|&b| b == b'/') {
            if comp.is_empty() {
                continue;
            }
            if !cur.is_dir() {
                return None;
            }
            cur = self.find_child(&cur, comp)?;
        }
        Some(cur)
    }

    fn find_child(&self, dir: &Inode, name: &[u8]) -> Option<Inode> {
        let mut found: Option<u32> = None;
        self.walk_dir(dir, |n, ino, _ft| {
            if found.is_none() && n == name {
                found = Some(ino);
            }
        });
        found.and_then(|n| self.read_inode(n))
    }

    /// Linear directory scan over the directory's data blocks. htree index
    /// blocks carry an inode-0 entry with a full-block rec_len and are skipped
    /// naturally; leaf blocks are ordinary entries.
    pub fn walk_dir(&self, dir: &Inode, mut f: impl FnMut(&[u8], u32, u8)) {
        let mut buf = [0u8; 4096];
        let limits = [(0u64, 0u32)];
        let _ = limits;
        let mut consumed = 0u64;
        // Iterate the directory's logical blocks through the extent tree.
        let mut buf2 = [0u8; 4096];
        while consumed < dir.size {
            let Some(block) = self.map_block(dir, consumed / self.block_size as u64) else {
                return;
            };
            if !self.read_block(block, &mut buf) {
                return;
            }
            let _ = &mut buf2;
            let mut off = 0usize;
            while off + 8 <= self.block_size as usize {
                let ino = le32(&buf, off);
                let rec_len = le16(&buf, off + 4) as usize;
                let name_len = buf[off + 6] as usize;
                if rec_len < 8 || off + rec_len > self.block_size as usize {
                    break;
                }
                if ino != 0 && name_len > 0 && name_len <= 255 && off + 8 + name_len <= self.block_size as usize {
                    f(&buf[off + 8..off + 8 + name_len], ino, buf[off + 7]);
                }
                off += rec_len;
            }
            consumed += self.block_size as u64;
        }
    }

    /// Read a file into BUF (truncated to the buffer); returns bytes read.
    pub fn read_file(&self, inode: &Inode, buf: &mut [u8]) -> Option<usize> {
        if !inode.is_file() && !inode.is_dir() {
            return None;
        }
        let want = (inode.size as usize).min(buf.len());
        let bs = self.block_size as u64;
        let mut got = 0usize;
        let mut blk = [0u8; 4096];
        while got < want {
            let logical = got as u64 / bs;
            let phys = self.map_block(inode, logical)?;
            if !self.read_block(phys, &mut blk) {
                return None;
            }
            let in_block = (got as u64 % bs) as usize;
            let n = (bs as usize - in_block).min(want - got);
            buf[got..got + n].copy_from_slice(&blk[in_block..in_block + n]);
            got += n;
        }
        Some(got)
    }

    /// Print a one-line summary (diagnostics + boot repair).
    pub fn describe(&self, s: &mut Serial) {
        let label = core::str::from_utf8(&self.label).unwrap_or("?").trim_end_matches('\0');
        let _ = writeln!(
            s,
            "ext4: mounted ro — uuid {}, {} blocks of {} B, {} inodes{}",
            guid_text(&self.uuid),
            self.blocks_count,
            self.block_size,
            self.inodes_count,
            if self.needs_recovery { " [needs journal recovery — read-only view]" } else { "" }
        );
        if !label.is_empty() {
            let _ = writeln!(s, "ext4: label '{}'", label);
        }
        let _ = serial::write_locked(b"");
    }
}

/// ext4 stores the UUID as a raw 16-byte value printed in the canonical
/// 8-4-4-4-12 hex form (unlike GPT, no byte swapping).
pub fn guid_text(uuid: &[u8; 16]) -> [u8; 36] {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut t = [0u8; 36];
    let mut o = 0;
    for (i, b) in uuid.iter().enumerate() {
        if i == 4 || i == 6 || i == 8 || i == 10 {
            t[o] = b'-';
            o += 1;
        }
        t[o] = HEX[(b >> 4) as usize];
        t[o + 1] = HEX[(b & 0xF) as usize];
        o += 2;
    }
    t
}
