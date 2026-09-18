//! Logical-to-physical block mapping: ext4 extent trees (depth 0/1 walked;
//! deeper trees are rejected) plus the classic ext2/3 block map for
//! filesystems without the extents feature. An uninitialised extent maps to
//! HOLE_BLOCK (reads as zeroes), never to disk data.

use crate::mem::to_usize;

use super::{le16, le32, Ext4, Inode, HOLE_BLOCK};

const EXTENT_MAGIC: u16 = 0xF30A;
const EH_SIZE: usize = 12;
const EE_SIZE: usize = 12;

impl Ext4 {
    /// Map a file's logical block to a physical filesystem block.
    pub fn map_block(&self, inode: &Inode, logical: u64) -> Option<u64> {
        if !inode.extents() {
            return self.classic_map(inode, logical);
        }

        // The root of the tree lives in the inode's 60-byte i_block area.
        let mut node = [0u8; 4096];
        let mut node_len = 60usize;
        node[..60].copy_from_slice(&inode.blocks);
        let mut hops = 0;

        loop {
            if le16(&node, 0) != EXTENT_MAGIC {
                return None;
            }
            let entries = le16(&node, 2) as usize;
            let depth = le16(&node, 6);
            let max = (node_len - EH_SIZE) / EE_SIZE;
            let n = entries.min(max);

            if depth == 0 {
                for i in 0..n {
                    let o = EH_SIZE + i * EE_SIZE;
                    let ee_block = le32(&node, o) as u64;
                    let raw_len = le16(&node, o + 4) as u64;
                    // bit 15 marks an uninitialised extent (reads as zeroes).
                    let len = raw_len & 0x7FFF;
                    let uninit = raw_len & 0x8000 != 0;
                    let start = ((le16(&node, o + 6) as u64) << 32) | le32(&node, o + 8) as u64;
                    if len > 0 && logical >= ee_block && logical < ee_block + len {
                        if uninit {
                            return Some(HOLE_BLOCK);
                        }
                        return Some(start + (logical - ee_block));
                    }
                }
                return None;
            }

            // Index node: descend into the last child whose range starts at
            // or before the requested logical block.
            let mut leaf = None;
            for i in 0..n {
                let o = EH_SIZE + i * EE_SIZE;
                if le32(&node, o) as u64 <= logical {
                    leaf = Some(((le16(&node, o + 8) as u64) << 32) | le32(&node, o + 4) as u64);
                }
            }
            let leaf = leaf?;
            if !self.read_block(leaf, &mut node) {
                return None;
            }
            node_len = self.block_size as usize;
            hops += 1;
            if hops > 4 {
                return None; // deeper than any sane rescue read needs
            }
        }
    }

    /// ext2/3 style: 12 direct blocks, then a single indirect block.
    fn classic_map(&self, inode: &Inode, logical: u64) -> Option<u64> {
        if logical < 12 {
            let v = le32(&inode.blocks, logical as usize * 4) as u64;
            return if v == 0 { None } else { Some(v) };
        }
        let indirect = le32(&inode.blocks, 12 * 4) as u64;
        if indirect == 0 {
            return None;
        }
        let idx = logical - 12;
        if (idx + 1) * 4 > self.block_size as u64 {
            return None;
        }
        let idx = to_usize(idx)?;
        let mut blk = [0u8; 4096];
        if !self.read_block(indirect, &mut blk) {
            return None;
        }
        let v = le32(&blk, idx * 4) as u64;
        if v == 0 {
            None
        } else {
            Some(v)
        }
    }
}
