//! ext4 path lookup, directory scan and file reads (linear; no htree search).

use crate::mem::to_usize;

use super::{le16, le32, Ext4, Inode, HOLE_BLOCK, MAX_BLOCK_SIZE};

impl Ext4 {
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
    /// naturally; the leaf blocks are ordinary entries (a linear scan of them
    /// finds every name, which is all a rescue read needs).
    pub fn walk_dir(&self, dir: &Inode, mut f: impl FnMut(&[u8], u32, u8)) {
        let bs = self.block_size as u64;
        let mut buf = [0u8; MAX_BLOCK_SIZE as usize];
        let mut consumed = 0u64;
        while consumed < dir.size {
            let Some(block) = self.map_block(dir, consumed / bs) else {
                return;
            };
            if block == HOLE_BLOCK {
                consumed += bs;
                continue;
            }
            if !self.read_block(block, &mut buf) {
                return;
            }
            let mut off = 0usize;
            while off + 8 <= self.block_size as usize {
                let ino = le32(&buf, off);
                let rec_len = le16(&buf, off + 4) as usize;
                let name_len = buf[off + 6] as usize;
                if rec_len < 8 || off + rec_len > self.block_size as usize {
                    break; // corrupt entry: stop before walking off the block
                }
                if ino != 0 && name_len > 0 && name_len <= 255 && off + 8 + name_len <= self.block_size as usize {
                    f(&buf[off + 8..off + 8 + name_len], ino, buf[off + 7]);
                }
                off += rec_len;
            }
            consumed += bs;
        }
    }

    /// Read a file into BUF (truncated to the buffer); returns bytes read.
    /// Uninitialised extents read as zeroes.
    pub fn read_file(&self, inode: &Inode, buf: &mut [u8]) -> Option<usize> {
        if !inode.is_file() && !inode.is_dir() {
            return None;
        }
        let want = to_usize(inode.size).map_or(buf.len(), |n| n.min(buf.len()));
        let bs = self.block_size as u64;
        let mut got = 0usize;
        let mut blk = [0u8; MAX_BLOCK_SIZE as usize];
        while got < want {
            let logical = got as u64 / bs;
            let phys = self.map_block(inode, logical)?;
            if phys == HOLE_BLOCK {
                blk.fill(0);
            } else if !self.read_block(phys, &mut blk) {
                return None;
            }
            let in_block = (got as u64 % bs) as usize;
            let n = (bs as usize - in_block).min(want - got);
            buf[got..got + n].copy_from_slice(&blk[in_block..in_block + n]);
            got += n;
        }
        Some(got)
    }
}
