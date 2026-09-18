//! ext4 superblock + group-descriptor parsing and inode reads.

use super::{le16, le32, read_bytes, Ext4, Inode, EXT4_MAGIC, MAX_BLOCK_SIZE};

impl Ext4 {
    /// Parse the superblock at the partition start (byte 1024) plus group
    /// descriptor 0. Returns None for anything this driver cannot read safely
    /// (bad magic, oversized block, zeroed counts, no group descriptor).
    pub fn parse(part_lba: u64) -> Option<Ext4> {
        let mut sb = [0u8; 1024];
        if !read_bytes(part_lba + 2, &mut sb) {
            return None;
        }
        if le16(&sb, 0x38) != EXT4_MAGIC {
            return None;
        }
        let log_block = le32(&sb, 0x18);
        if log_block > 2 {
            return None; // 1..4 KiB blocks: bigger needs buffers we do not hold
        }
        let block_size = 1024u32 << log_block;
        let blocks_count = le32(&sb, 0x04) as u64;
        let inodes_count = le32(&sb, 0x00);
        let inodes_per_group = le32(&sb, 0x28);
        if blocks_count == 0 || inodes_count == 0 || inodes_per_group == 0 {
            return None; // magic-only fixture or a trashed superblock
        }
        let rev = le32(&sb, 0x4C);
        let inode_size = if rev == 0 { 128 } else { le16(&sb, 0x58) as u32 };
        if inode_size < 128 || inode_size > block_size {
            return None;
        }
        let incompat = le32(&sb, 0x60);
        // 64-bit feature: group descriptors are 64 bytes and s_desc_size is
        // authoritative; without it they are the classic 32.
        let desc_size = if incompat & 0x80 != 0 { le16(&sb, 0xFE) as u32 } else { 32 };
        if desc_size < 32 || desc_size > block_size {
            return None;
        }

        let mut uuid = [0u8; 16];
        uuid.copy_from_slice(&sb[0x68..0x78]);
        let mut label = [0u8; 16];
        label.copy_from_slice(&sb[0x78..0x88]);

        let fs = Ext4 {
            part_lba,
            block_size,
            blocks_count,
            inodes_count,
            inodes_per_group,
            inode_size,
            desc_size,
            inode_table_block: 0,
            uuid,
            label,
            needs_recovery: incompat & 0x0004 != 0,
        };

        // Group descriptor 0 sits in the block after the superblock.
        let first_data = if block_size == 1024 { 1u64 } else { 0 };
        let mut gdt = [0u8; MAX_BLOCK_SIZE as usize];
        if !fs.read_block(first_data + 1, &mut gdt) {
            return None;
        }
        let inode_table = le32(&gdt, 0x08) as u64;
        if inode_table == 0 {
            return None;
        }
        Some(Ext4 { inode_table_block: inode_table, ..fs })
    }

    /// Read inode NUM (1-based) from its group's inode table.
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
            let mut gdt = [0u8; MAX_BLOCK_SIZE as usize];
            let byte = group as u64 * self.desc_size as u64;
            let block = first_data + 1 + byte / self.block_size as u64;
            if !self.read_block(block, &mut gdt) {
                return None;
            }
            le32(&gdt, (byte % self.block_size as u64) as usize + 0x08) as u64
        };
        if table_block == 0 {
            return None;
        }

        let byte_off = index as u64 * self.inode_size as u64;
        let block = table_block + byte_off / self.block_size as u64;
        let mut buf = [0u8; MAX_BLOCK_SIZE as usize];
        if !self.read_block(block, &mut buf) {
            return None;
        }
        let o = (byte_off % self.block_size as u64) as usize;
        if o + self.inode_size as usize > buf.len() {
            return None;
        }
        let mut blocks = [0u8; 60];
        blocks.copy_from_slice(&buf[o + 40..o + 100]);
        let size_lo = le32(&buf, o + 4) as u64;
        // i_size_high exists once the inode is bigger than the classic 128.
        let size_high = if self.inode_size > 108 { le32(&buf, o + 108) as u64 } else { 0 };
        Some(Inode {
            mode: le16(&buf, o),
            size: size_lo | (size_high << 32),
            flags: le32(&buf, o + 32),
            blocks,
        })
    }

    pub fn root(&self) -> Option<Inode> {
        self.read_inode(2)
    }
}
