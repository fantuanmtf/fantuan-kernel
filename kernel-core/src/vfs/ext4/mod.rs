//! ext4 read-only driver (M6.5, DESIGN.md §8/§9): superblock + group
//! descriptors, inodes with extent trees, linear directory scan and file
//! reads. Read-only by construction — no journal replay, no writes, no
//! allocation; a filesystem that needs recovery is reported as such.
//!
//! Module map:
//!   - super.rs    superblock/GDT parse, inode reads
//!   - dir.rs      path lookup, directory scan, file reads
//!   - extents.rs  logical-to-physical block mapping (extents + classic map)
//!
//! Scope: the fields a rescue system needs — UUID, label, block size, inode
//! and block counts, path lookup, directory listing, file read. Block sizes
//! up to 4 KiB (bigger needs buffers no kernel stack can hold), extent trees
//! to depth 0/1, 64-byte group descriptors understood. Out of scope: inline
//! data, htree-indexed directory search (linear scan still works), journal
//! replay.

mod dir;
mod extents;
mod sb;

use core::ffi::c_void;
use core::fmt::Write;

use crate::log::Log;
use crate::mem::to_usize;

extern "C" {
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
}

const EXT4_MAGIC: u16 = 0xEF53;
const S_IFDIR: u16 = 0x4000;
const S_IFREG: u16 = 0x8000;
pub const EXT4_EXTENTS_FL: u32 = 0x0008_0000;
/// The largest block this driver buffers; `parse` rejects bigger ones.
const MAX_BLOCK_SIZE: u32 = 4096;
/// `map_block` hole sentinel: an uninitialised extent reads as zeroes, and
/// block 0 is never file data (it holds the boot/superblock area).
const HOLE_BLOCK: u64 = 0;

/// One inode as far as the rescue kernel cares.
#[derive(Clone, Copy)]
pub struct Inode {
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

/// A mounted ext4 filesystem (read-only).
#[derive(Clone, Copy)]
pub struct Ext4 {
    /// Partition start, in 512-byte sectors.
    pub part_lba: u64,
    pub block_size: u32,
    pub blocks_count: u64,
    pub inodes_count: u32,
    pub inodes_per_group: u32,
    pub inode_size: u32,
    /// Group-descriptor size: 32, or 64 with the 64bit feature.
    pub desc_size: u32,
    /// Group 0's inode-table block (the common case cached at parse time).
    pub inode_table_block: u64,
    pub uuid: [u8; 16],
    pub label: [u8; 16],
    pub needs_recovery: bool,
}

// --- little-endian field readers (the on-disk format is LE) ---------------

fn le16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

// --- block I/O -------------------------------------------------------------

/// Read raw sectors starting at LBA (used for the 1024-byte superblock read,
/// which is not block-aligned on 4 KiB filesystems).
fn read_bytes(lba: u64, buf: &mut [u8]) -> bool {
    let sectors = buf.len() / 512;
    if sectors == 0 || buf.len() % 512 != 0 {
        return false;
    }
    unsafe { blk_read(core::ptr::null_mut(), lba, buf.as_mut_ptr() as *mut c_void, sectors) == 0 }
}

impl Ext4 {
    /// Read COUNT consecutive blocks into BUF (which must hold the full block).
    fn read_blocks(&self, block: u64, count: u32, buf: &mut [u8]) -> bool {
        let block_sectors = self.block_size as u64 / 512;
        let lba = self.part_lba + block * block_sectors;
        let Some(sectors) = to_usize(count as u64 * block_sectors) else { return false };
        if buf.len() < sectors * 512 {
            return false;
        }
        unsafe { blk_read(core::ptr::null_mut(), lba, buf.as_mut_ptr() as *mut c_void, sectors) == 0 }
    }

    fn read_block(&self, block: u64, buf: &mut [u8]) -> bool {
        self.read_blocks(block, 1, buf)
    }

    /// Print a one-line summary (diagnostics + boot repair).
    pub fn describe(&self, s: &mut Log) {
        let label = core::str::from_utf8(&self.label).unwrap_or("?").trim_end_matches('\0');
        let uuid = guid_text(&self.uuid);
        let uuid_text = core::str::from_utf8(&uuid).unwrap_or("?");
        let _ = writeln!(
            s,
            "ext4: mounted ro — uuid {}, {} blocks of {} B, {} inodes{}",
            uuid_text,
            self.blocks_count,
            self.block_size,
            self.inodes_count,
            if self.needs_recovery { " [needs journal recovery — read-only view]" } else { "" }
        );
        if !label.is_empty() {
            let _ = writeln!(s, "ext4: label '{}'", label);
        }
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
