//! FAT32 read-only driver (M6, DESIGN.md §8): BPB parse, FAT chain walk,
//! 8.3 directory entries (LFN skipped), file read. Pure Rust filesystem
//! logic on top of the C block driver.

use core::ffi::c_void;

extern "C" {
    fn blk_read(dev: *mut c_void, lba: u64, buf: *mut c_void, sectors: usize) -> i32;
}

const EOC_MIN: u32 = 0x0FFF_FFF8;

#[derive(Clone, Copy)]
pub struct Fat32 {
    pub sectors_per_cluster: u32,
    pub sectors_per_fat: u32,
    /// Number of FAT copies from the BPB — writers must update all of them.
    pub num_fats: u32,
    pub root_cluster: u32,
    pub fat_lba: u64,
    pub data_lba: u64,
}

fn read_sector(lba: u64, buf: &mut [u8; 512]) -> bool {
    unsafe { blk_read(core::ptr::null_mut(), lba, buf.as_mut_ptr() as *mut c_void, 1) == 0 }
}

/// Parse the BPB at the partition's first sector.
pub fn parse(part_lba: u64) -> Option<Fat32> {
    let mut bpb = [0u8; 512];
    if !read_sector(part_lba, &mut bpb) {
        return None;
    }
    if bpb[510] != 0x55 || bpb[511] != 0xAA {
        return None;
    }
    let bytes_per_sector = u16::from_le_bytes([bpb[11], bpb[12]]) as u32;
    if bytes_per_sector != 512 {
        return None; // M6 scope: 512-byte sectors only
    }
    let spc = bpb[13] as u32;
    let reserved = u16::from_le_bytes([bpb[14], bpb[15]]) as u32;
    let num_fats = bpb[16] as u32;
    let spf = u32::from_le_bytes([bpb[36], bpb[37], bpb[38], bpb[39]]);
    let root_cluster = u32::from_le_bytes([bpb[44], bpb[45], bpb[46], bpb[47]]);
    if num_fats == 0 || num_fats > 4 {
        return None;
    }
    let fat_lba = part_lba + reserved as u64;
    let data_lba = fat_lba + (num_fats * spf) as u64;
    Some(Fat32 {
        sectors_per_cluster: spc,
        sectors_per_fat: spf,
        num_fats,
        root_cluster,
        fat_lba,
        data_lba,
    })
}

impl Fat32 {
    fn cluster_to_lba(&self, n: u32) -> u64 {
        self.data_lba + (n as u64 - 2) * self.sectors_per_cluster as u64
    }

    /// Read the FAT entry for cluster n (follow the chain).
    pub fn next_cluster(&self, n: u32) -> u32 {
        let entry_offset = n as u64 * 4;
        let mut sec = [0u8; 512];
        if !read_sector(self.fat_lba + entry_offset / 512, &mut sec) {
            return EOC_MIN;
        }
        let off = (entry_offset % 512) as usize;
        u32::from_le_bytes([sec[off], sec[off + 1], sec[off + 2], sec[off + 3]]) & 0x0FFF_FFFF
    }

    /// Read a whole file into buf; returns the bytes copied.
    pub fn read_file(&self, start_cluster: u32, size: u32, buf: &mut [u8]) -> Option<usize> {
        let mut got = 0usize;
        let mut c = start_cluster;
        let mut sec = [0u8; 512];
        while got < size as usize && c >= 2 && c < EOC_MIN {
            let lba = self.cluster_to_lba(c);
            let cluster_bytes = (self.sectors_per_cluster * 512) as usize;
            let mut done = 0usize;
            while done < cluster_bytes && got < size as usize {
                if !read_sector(lba + (done / 512) as u64, &mut sec) {
                    return None;
                }
                let in_sec = done % 512;
                let n = (512 - in_sec).min(cluster_bytes - done).min(size as usize - got);
                buf[got..got + n].copy_from_slice(&sec[in_sec..in_sec + n]);
                got += n;
                done += n;
            }
            c = self.next_cluster(c);
        }
        Some(got)
    }

    /// Read LEN bytes starting at byte OFFSET of a file (used for streaming
    /// copies that cannot hold the whole file in one buffer).
    pub fn read_range(&self, start_cluster: u32, offset: u64, buf: &mut [u8]) -> Option<usize> {
        let cluster_bytes = (self.sectors_per_cluster * 512) as u64;
        let mut skip = offset / cluster_bytes;
        let mut c = start_cluster;
        while skip > 0 && c >= 2 && c < EOC_MIN {
            c = self.next_cluster(c);
            skip -= 1;
        }
        if skip > 0 || c < 2 || c >= EOC_MIN {
            return None;
        }
        let mut in_cluster = offset % cluster_bytes;
        let mut got = 0usize;
        let mut sec = [0u8; 512];
        while got < buf.len() && c >= 2 && c < EOC_MIN {
            let lba = self.cluster_to_lba(c);
            while in_cluster < cluster_bytes && got < buf.len() {
                let sec_idx = in_cluster / 512;
                if !read_sector(lba + sec_idx, &mut sec) {
                    return None;
                }
                let in_sec = (in_cluster % 512) as usize;
                let n = (512 - in_sec).min((cluster_bytes - in_cluster) as usize).min(buf.len() - got);
                buf[got..got + n].copy_from_slice(&sec[in_sec..in_sec + n]);
                got += n;
                in_cluster += n as u64;
            }
            if got >= buf.len() {
                break;
            }
            in_cluster = 0;
            c = self.next_cluster(c);
        }
        Some(got)
    }

    /// Walk a directory's 8.3 entries (LFN and deleted entries skipped).
    pub fn walk_dir(&self, start_cluster: u32, mut f: impl FnMut(&[u8; 11], u8, u32, u32)) {
        let mut c = start_cluster;
        let mut sec = [0u8; 512];
        while c >= 2 && c < EOC_MIN {
            let lba = self.cluster_to_lba(c);
            for s in 0..self.sectors_per_cluster {
                if !read_sector(lba + s as u64, &mut sec) {
                    return;
                }
                for off in (0..512).step_by(32) {
                    let e = &sec[off..off + 32];
                    let name: [u8; 11] = e[0..11].try_into().unwrap();
                    let attr = e[11];
                    if name[0] == 0x00 {
                        return; // end of directory
                    }
                    if name[0] == 0xE5 || attr == 0x0F {
                        continue; // deleted / LFN entry
                    }
                    let hi = u16::from_le_bytes([e[20], e[21]]) as u32;
                    let lo = u16::from_le_bytes([e[26], e[27]]) as u32;
                    let size = u32::from_le_bytes([e[28], e[29], e[30], e[31]]);
                    f(&name, attr, (hi << 16) | lo, size);
                }
            }
            c = self.next_cluster(c);
        }
    }
}
