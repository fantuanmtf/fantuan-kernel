//! FAT32 write path (M7.5b; streaming since the P0 audit): cluster
//! allocation, FAT updates across every FAT copy, data writes and directory
//! entries. Every public entry point takes a `RepairToken` (vfs::repair_guard)
//! — the rescue iron rule, enforced at the type level.
//!
//! Metadata ordering: a cluster's data is on disk before any FAT entry points
//! at it, and the directory entry is written last — so an interrupted write
//! never exposes a chain that leads to garbage. The streaming writer keeps no
//! chain array, so file size is bounded by the disk, not by a stack buffer.

use core::ffi::c_void;

use super::fat::Fat32;
use super::RepairToken;

extern "C" {
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
    fn blk_write(dev: *mut c_void, lba: u64, buf: *const c_void, sectors: usize) -> i32;
}

const EOC: u32 = 0x0FFF_FFFF;
const EOC_MIN: u32 = 0x0FFF_FFF8;
/// Largest cluster this writer buffers (FAT32 allows up to 64 KiB).
pub const MAX_CLUSTER_BYTES: usize = 32 * 1024;

fn write_sector(lba: u64, buf: &[u8; 512]) -> bool {
    unsafe { blk_write(core::ptr::null_mut(), lba, buf.as_ptr() as *const c_void, 1) == 0 }
}

fn read_sector(lba: u64, buf: &mut [u8; 512]) -> bool {
    unsafe { blk_read(core::ptr::null_mut(), lba, buf.as_mut_ptr() as *mut c_void, 1) == 0 }
}

/// Append-only writer for one file: chunks are streamed in, the chain is
/// linked as it grows, and finish() writes the directory entry.
pub struct FileWriter<'a> {
    fs: &'a Fat32,
    dir_cluster: u32,
    name: [u8; 11],
    first: u32,
    prev: u32,
    size: u32,
    cluster_bytes: usize,
}

impl Fat32 {
    /// Update the FAT entry for cluster n in EVERY FAT copy.
    fn write_fat_entry(&self, n: u32, value: u32) -> bool {
        let entry_offset = n as u64 * 4;
        let rel_sector = (entry_offset / 512) as u32;
        let off = (entry_offset % 512) as usize;
        for fat_index in 0..self.num_fats {
            let lba = self.fat_lba + (fat_index * self.sectors_per_fat + rel_sector) as u64;
            let mut sec = [0u8; 512];
            if !read_sector(lba, &mut sec) {
                return false;
            }
            sec[off..off + 4].copy_from_slice(&value.to_le_bytes());
            if !write_sector(lba, &sec) {
                return false;
            }
        }
        true
    }

    /// Find a free cluster (zero FAT entry), mark it EOC, return it.
    pub(super) fn alloc_cluster(&self) -> Option<u32> {
        let mut sec = [0u8; 512];
        for s in 0..self.sectors_per_fat {
            let lba = self.fat_lba + s as u64;
            if !read_sector(lba, &mut sec) {
                return None;
            }
            for off in (0..512).step_by(4) {
                let entry = u32::from_le_bytes([sec[off], sec[off + 1], sec[off + 2], sec[off + 3]]);
                if entry == 0 {
                    let n = s * 128 + (off / 4) as u32;
                    if n >= 2 {
                        if !self.write_fat_entry(n, EOC) {
                            return None; // a free cluster we cannot mark is unusable
                        }
                        return Some(n);
                    }
                }
            }
        }
        None
    }

    /// Write one cluster's worth of data (zero-padded past the end). DATA may
    /// be shorter than the cluster; sectors entirely past its end are written
    /// as zeroes (the `start < data.len()` guard keeps the slice in range).
    pub(super) fn write_cluster(&self, n: u32, data: &[u8]) -> bool {
        let cluster_bytes = (self.sectors_per_cluster * 512) as usize;
        let lba = self.data_lba + (n as u64 - 2) * self.sectors_per_cluster as u64;
        let mut sec = [0u8; 512];
        for s in 0..self.sectors_per_cluster {
            sec.fill(0);
            let start = (s * 512) as usize;
            if start < data.len() {
                let take = (data.len() - start).min(512).min(cluster_bytes - start);
                sec[..take].copy_from_slice(&data[start..start + take]);
            }
            if !write_sector(lba + s as u64, &sec) {
                return false;
            }
        }
        true
    }

    /// Start a streaming file. Call append() for the payload (data, then the
    /// link into the previous cluster) and finish() to publish the entry.
    /// TOKEN is proof that repair mode is on (vfs::repair_guard).
    pub fn create_file(&self, dir_cluster: u32, name: &[u8; 11], _token: &RepairToken) -> Option<FileWriter<'_>> {
        let cluster_bytes = (self.sectors_per_cluster * 512) as usize;
        if cluster_bytes == 0 || cluster_bytes > MAX_CLUSTER_BYTES {
            return None;
        }
        Some(FileWriter {
            fs: self,
            dir_cluster,
            name: *name,
            first: 0,
            prev: 0,
            size: 0,
            cluster_bytes,
        })
    }

    /// Create (or overwrite) a file from one buffer — the simple path used by
    /// the self-test. Large copies should stream through create_file().
    pub fn write_file(&self, dir_cluster: u32, name: &[u8; 11], data: &[u8], token: &RepairToken) -> bool {
        let Some(mut w) = self.create_file(dir_cluster, name, token) else {
            return false;
        };
        if data.is_empty() {
            // A zero-length file still needs a start cluster for its entry.
            let Some(c) = self.alloc_cluster() else {
                return false;
            };
            w.first = c;
            w.prev = c;
        } else if !w.append(data) {
            return false;
        }
        w.finish()
    }

    /// Put an 8.3 entry into the directory: overwrite the existing entry of
    /// the same name in place, otherwise fill a deleted/end slot. Both passes
    /// are capped at the FAT chain limit, so a cyclic directory never hangs
    /// the writer. ATTR is the attribute byte (0x20 file, 0x10 directory).
    pub(super) fn append_dir_entry(&self, dir_cluster: u32, name: &[u8; 11], cluster: u32, size: u32, attr: u8) -> bool {
        // Pass 1: an entry of the same name is overwritten in place, so
        // repeated boots never create duplicates.
        let mut c = dir_cluster;
        let mut hops = 0u64;
        let mut sec = [0u8; 512];
        while c >= 2 && c < EOC_MIN {
            hops += 1;
            if hops > self.chain_limit() {
                return false;
            }
            let lba = self.data_lba + (c as u64 - 2) * self.sectors_per_cluster as u64;
            for s in 0..self.sectors_per_cluster {
                if !read_sector(lba + s as u64, &mut sec) {
                    return false;
                }
                for off in (0..512).step_by(32) {
                    if sec[off] == 0x00 {
                        break;
                    }
                    if sec[off] == 0xE5 {
                        continue;
                    }
                    if sec[off..off + 11] == name[..] {
                        sec[off + 20..off + 22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
                        sec[off + 26..off + 28].copy_from_slice(&(cluster as u16).to_le_bytes());
                        sec[off + 28..off + 32].copy_from_slice(&size.to_le_bytes());
                        return write_sector(lba + s as u64, &sec);
                    }
                }
            }
            c = self.next_cluster(c);
        }
        // Pass 2: first deleted (0xE5) or end-of-directory (0x00) slot.
        c = dir_cluster;
        hops = 0;
        while c >= 2 && c < EOC_MIN {
            hops += 1;
            if hops > self.chain_limit() {
                return false;
            }
            let lba = self.data_lba + (c as u64 - 2) * self.sectors_per_cluster as u64;
            for s in 0..self.sectors_per_cluster {
                if !read_sector(lba + s as u64, &mut sec) {
                    return false;
                }
                for off in (0..512).step_by(32) {
                    if sec[off] == 0x00 || sec[off] == 0xE5 {
                        let mut e = [0u8; 32];
                        e[0..11].copy_from_slice(name);
                        e[11] = attr;
                        e[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
                        e[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
                        e[28..32].copy_from_slice(&size.to_le_bytes());
                        sec[off..off + 32].copy_from_slice(&e);
                        return write_sector(lba + s as u64, &sec);
                    }
                }
            }
            c = self.next_cluster(c);
        }
        false
    }
}

impl FileWriter<'_> {
    /// Number of bytes written so far.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Append a chunk: each cluster's data goes to disk first, then the
    /// previous cluster is linked to it. Chunks may be any size.
    pub fn append(&mut self, data: &[u8]) -> bool {
        for chunk in data.chunks(self.cluster_bytes) {
            let Some(c) = self.fs.alloc_cluster() else {
                return false;
            };
            if !self.fs.write_cluster(c, chunk) {
                return false;
            }
            if self.first == 0 {
                self.first = c;
            } else if !self.fs.write_fat_entry(self.prev, c) {
                return false;
            }
            self.prev = c;
            self.size += chunk.len() as u32;
        }
        true
    }

    /// Publish the directory entry. The last cluster already carries EOC.
    pub fn finish(self) -> bool {
        if self.first == 0 {
            return false;
        }
        self.fs.append_dir_entry(self.dir_cluster, &self.name, self.first, self.size, 0x20 /* archive */)
    }
}
