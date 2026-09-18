//! FAT32 directory creation (M7.9): allocate a cluster, write its dot/dotdot
//! entries and publish the subdirectory entry in the parent. Every entry
//! point takes a `RepairToken` (vfs::repair_guard) — the rescue iron rule.
//! Needs the parent directory (the ESP tree is 8.3; names are short here).

use super::fat::Fat32;
use super::RepairToken;

const ATTR_DIR: u8 = 0x10;

/// Build the first cluster of a new directory: "." (self) and ".." (parent).
fn dot_entries(self_cluster: u32, parent_cluster: u32) -> [u8; 64] {
    let mut buf = [0u8; 64];
    // "." at offset 0: name ".          ", attr dir.
    buf[0..11].fill(b' ');
    buf[0] = b'.';
    buf[11] = ATTR_DIR;
    buf[20..22].copy_from_slice(&((self_cluster >> 16) as u16).to_le_bytes());
    buf[26..28].copy_from_slice(&(self_cluster as u16).to_le_bytes());
    // ".." at offset 32.
    buf[32..43].fill(b' ');
    buf[32] = b'.';
    buf[33] = b'.';
    buf[43] = ATTR_DIR;
    buf[52..54].copy_from_slice(&((parent_cluster >> 16) as u16).to_le_bytes());
    buf[58..60].copy_from_slice(&(parent_cluster as u16).to_le_bytes());
    buf
}

impl Fat32 {
    /// Create a NEW directory NAME under DIR_CLUSTER and return its cluster.
    /// The caller must have checked that the name does not exist.
    pub fn create_dir(&self, dir_cluster: u32, name: &[u8; 11], _token: &RepairToken) -> Option<u32> {
        let cluster = self.alloc_cluster()?;
        let dot = dot_entries(cluster, dir_cluster);
        if !self.write_cluster(cluster, &dot) {
            return None;
        }
        if !self.append_dir_entry(dir_cluster, name, cluster, 0, ATTR_DIR) {
            return None;
        }
        Some(cluster)
    }

    /// Find NAME under PARENT, creating it as a directory when absent.
    /// Returns None when the name is taken by a non-directory or a write
    /// fails.
    pub fn find_or_create_dir(&self, parent: u32, name: &[u8; 11], token: &RepairToken) -> Option<u32> {
        let mut existing: Option<(u32, bool)> = None;
        self.walk_dir(parent, |n, attr, cluster, _size| {
            if existing.is_none() && super::eq_8_3(n, name) {
                existing = Some((cluster, attr & ATTR_DIR != 0));
            }
        });
        match existing {
            Some((cluster, true)) => Some(cluster),
            Some((_, false)) => None, // a file owns this name
            None => self.create_dir(parent, name, token),
        }
    }
}
