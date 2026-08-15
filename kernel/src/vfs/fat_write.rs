//! FAT32 write path (M7.5b): cluster allocation, FAT updates (both copies),
//! data writes and directory-entry writes. The kernel gates every call
//! behind repair mode (vfs::write_file) — the rescue iron rule.
//!
//! Metadata ordering: data clusters first, then the FAT chain, then the
//! directory entry — so an interrupted write never points at garbage.

use core::ffi::c_void;

use super::fat::Fat32;

extern "C" {
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
    fn blk_write(dev: *mut c_void, lba: u64, buf: *const c_void, sectors: usize) -> i32;
}

const EOC: u32 = 0x0FFF_FFFF;
const EOC_MIN: u32 = 0x0FFF_FFF8;

fn write_sector(lba: u64, buf: &[u8; 512]) -> bool {
    unsafe { blk_write(core::ptr::null_mut(), lba, buf.as_ptr() as *const c_void, 1) == 0 }
}

fn read_sector(lba: u64, buf: &mut [u8; 512]) -> bool {
    unsafe { blk_read(core::ptr::null_mut(), lba, buf.as_mut_ptr() as *mut c_void, 1) == 0 }
}

impl Fat32 {
    /// Update the FAT entry for cluster n in BOTH FAT copies.
    fn write_fat_entry(&self, n: u32, value: u32) -> bool {
        let entry_offset = n as u64 * 4;
        let rel_sector = (entry_offset / 512) as u32;
        let off = (entry_offset % 512) as usize;
        for fat_index in 0..2u32 {
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
    fn alloc_cluster(&self) -> Option<u32> {
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
                        self.write_fat_entry(n, EOC);
                        return Some(n);
                    }
                }
            }
        }
        None
    }

    /// Write one cluster's worth of data (zero-padded past the end).
    fn write_cluster(&self, n: u32, data: &[u8]) -> bool {
        let cluster_bytes = (self.sectors_per_cluster * 512) as usize;
        let lba = self.data_lba + (n as u64 - 2) * self.sectors_per_cluster as u64;
        let mut sec = [0u8; 512];
        for s in 0..self.sectors_per_cluster {
            sec.fill(0);
            let start = (s * 512) as usize;
            let take = data.len().saturating_sub(start).min(512).min(cluster_bytes - start);
            sec[..take].copy_from_slice(&data[start..start + take]);
            if !write_sector(lba + s as u64, &sec) {
                return false;
            }
        }
        true
    }

    /// Create (or overwrite) an 8.3 file in the given directory cluster.
    /// Returns true on success.
    pub fn write_file(&self, dir_cluster: u32, name: &[u8; 11], data: &[u8]) -> bool {
        let cluster_bytes = (self.sectors_per_cluster * 512) as usize;
        let clusters_needed = data.len().div_ceil(cluster_bytes).max(1) as u32;

        // 1. Allocate the chain + write the data (data first).
        let mut chain = [0u32; 16];
        if clusters_needed as usize > chain.len() {
            return false;
        }
        for i in 0..clusters_needed {
            let Some(c) = self.alloc_cluster() else {
                return false;
            };
            chain[i as usize] = c;
        }
        for i in 0..clusters_needed {
            let start = i as usize * cluster_bytes;
            let end = (start + cluster_bytes).min(data.len());
            if !self.write_cluster(chain[i as usize], &data[start..end]) {
                return false;
            }
        }

        // 2. FAT chain (second, after the data is on disk).
        for i in 0..clusters_needed {
            let next = if i + 1 < clusters_needed { chain[i as usize + 1] } else { EOC };
            if !self.write_fat_entry(chain[i as usize], next) {
                return false;
            }
        }

        // 3. Directory entry (last).
        self.append_dir_entry(dir_cluster, name, chain[0], data.len() as u32)
    }

    /// Put an 8.3 entry into the directory: overwrite the existing entry of
    /// the same name in place, otherwise fill a deleted/end slot.
    fn append_dir_entry(&self, dir_cluster: u32, name: &[u8; 11], cluster: u32, size: u32) -> bool {
        // Pass 1: an entry of the same name is overwritten in place, so
        // repeated boots never create duplicates.
        let mut c = dir_cluster;
        let mut sec = [0u8; 512];
        while c >= 2 && c < EOC_MIN {
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
        while c >= 2 && c < EOC_MIN {
            let lba = self.data_lba + (c as u64 - 2) * self.sectors_per_cluster as u64;
            for s in 0..self.sectors_per_cluster {
                if !read_sector(lba + s as u64, &mut sec) {
                    return false;
                }
                for off in (0..512).step_by(32) {
                    if sec[off] == 0x00 || sec[off] == 0xE5 {
                        let mut e = [0u8; 32];
                        e[0..11].copy_from_slice(name);
                        e[11] = 0x20; // archive
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
