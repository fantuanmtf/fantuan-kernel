#!/usr/bin/env python3
"""mkdisk_ext4.py — hand-built minimal ext4 root fixture for tools/mkdisk.py.

1 KiB blocks, one group, extent trees on every inode, root with /etc/fstab,
/etc/os-release, /etc/default/grub, /hello.txt and /boot/vmlinuz+initrd (the
M6.5 read path and the M7.9 repair inventory). No journal, no checksums, no
backup superblocks — just enough for the read-only rescue driver. Split out
of mkdisk.py to keep every file inside the size rule; not a standalone tool.
"""

import struct

SECTOR = 512
HELLO_EXT4 = b"hello from the ext4 root\n"
OSREL = b'ID=fantuan\nVERSION_ID="6.6"\nPRETTY_NAME="Fantuan Test Linux"\n'
GRUB_DEFAULT = b'GRUB_DEFAULT=0\nGRUB_CMDLINE_LINUX="quiet splash"\n'
VMLINUZ = b"FANTUAN TEST KERNEL IMAGE\n" * 4
INITRD = b"FANTUAN TEST INITRD IMAGE\n" * 4


def build(out, part_lba, fstab):
    """Hand-built minimal ext4: 1 KiB blocks, one group, extent trees on every
    inode, root with /etc/fstab, /etc/os-release, /etc/default/grub,
    /hello.txt and /boot/vmlinuz+initrd (the M7.9 repair inventory). No
    journal, no checksums, no backup superblocks — just enough for the
    read-only rescue driver."""
    def wblock(lba, data):
        """Write a multi-sector block at LBA (wsect is 512 bytes only)."""
        out[lba * SECTOR:lba * SECTOR + len(data)] = data

    BLK = 1024
    N_BLOCKS = 64
    N_INODES = 16
    # Block plan (blocks are 1 KiB, i.e. 2 sectors).
    (B_SB, B_GDT, B_BBITMAP, B_IBITMAP, B_ITABLE, B_ROOT, B_ETC, B_fstab,
     B_HELLO, B_BOOT, B_VMLINUZ, B_INITRD, B_OSREL, B_ETCDEF, B_DEFGRUB) = (
        1, 2, 3, 4, 5, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16)
    USED = 17  # blocks 0..16

    def w16(b, o, v):
        b[o:o + 2] = struct.pack('<H', v)

    def w32(b, o, v):
        b[o:o + 4] = struct.pack('<I', v)

    sb = bytearray(BLK)
    w32(sb, 0x00, N_INODES)
    w32(sb, 0x04, N_BLOCKS)
    w32(sb, 0x0C, N_BLOCKS - USED)
    w32(sb, 0x10, N_INODES - 11)
    w32(sb, 0x14, 1)            # first_data_block (1 KiB blocks)
    w32(sb, 0x18, 0)            # log_block_size -> 1024
    w32(sb, 0x20, N_BLOCKS)     # blocks_per_group
    w32(sb, 0x24, N_BLOCKS)
    w32(sb, 0x28, N_INODES)     # inodes_per_group
    w16(sb, 0x38, 0xEF53)       # magic
    w16(sb, 0x3A, 1)            # state: clean
    w32(sb, 0x4C, 1)            # rev_level 1: dynamic inodes
    w16(sb, 0x54, 11)           # first_ino
    w16(sb, 0x58, 128)          # inode_size
    w32(sb, 0x5C, 0x40)         # feature_compat: EXTENTS
    w32(sb, 0x60, 0x2)          # feature_incompat: FILETYPE
    sb[0x68:0x78] = bytes.fromhex("12345678123412341234123456789abc")
    sb[0x78:0x87] = b"FANTUANROOT"
    w16(sb, 0xFE, 32)           # s_desc_size (only meaningful with 64bit)
    wblock(part_lba + B_SB * 2, sb)

    gdt = bytearray(BLK)
    w32(gdt, 0x00, B_BBITMAP)
    w32(gdt, 0x04, B_IBITMAP)
    w32(gdt, 0x08, B_ITABLE)
    w16(gdt, 0x0C, N_BLOCKS - USED)
    w16(gdt, 0x0E, N_INODES - 11)
    w16(gdt, 0x10, 4)           # used_dirs (root, etc, boot, default)
    wblock(part_lba + B_GDT * 2, gdt)

    bbitmap = bytearray(BLK)
    for b in range(USED):
        bbitmap[b // 8] |= 1 << (b % 8)
    wblock(part_lba + B_BBITMAP * 2, bbitmap)

    ibitmap = bytearray(BLK)
    for i in range(1, 12):      # inodes 1..11 used
        ibitmap[(i - 1) // 8] |= 1 << ((i - 1) % 8)
    wblock(part_lba + B_IBITMAP * 2, ibitmap)

    itable = bytearray(2 * BLK)

    def inode(slot, mode, size, links, block):
        o = (slot - 1) * 128
        w16(itable, o + 0x00, mode)
        w32(itable, o + 0x04, size & 0xFFFFFFFF)
        w16(itable, o + 0x1A, links)
        w32(itable, o + 0x1C, (size + 511) // 512)   # i_blocks in 512B units
        w32(itable, o + 0x20, 0x80000)               # EXT4_EXTENTS_FL
        # extent header (i_block): magic, 1 entry, max 4, depth 0.
        ib = o + 0x28
        itable[ib:ib + 2] = struct.pack('<H', 0xF30A)
        itable[ib + 2:ib + 4] = struct.pack('<H', 1)
        itable[ib + 4:ib + 6] = struct.pack('<H', 4)
        itable[ib + 6:ib + 8] = struct.pack('<H', 0)
        # leaf extent: ee_block=0, ee_len=1, ee_start.
        itable[ib + 12:ib + 16] = struct.pack('<I', 0)
        itable[ib + 16:ib + 18] = struct.pack('<H', 1)
        itable[ib + 18:ib + 20] = struct.pack('<H', 0)
        itable[ib + 20:ib + 24] = struct.pack('<I', block)

    inode(2, 0x41ED, BLK, 3, B_ROOT)
    inode(3, 0x41ED, BLK, 2, B_ETC)
    inode(4, 0x81A4, len(fstab), 1, B_fstab)
    inode(5, 0x81A4, len(HELLO_EXT4), 1, B_HELLO)
    inode(6, 0x41ED, BLK, 2, B_BOOT)          # /boot
    inode(7, 0x81A4, len(VMLINUZ), 1, B_VMLINUZ)
    inode(8, 0x81A4, len(INITRD), 1, B_INITRD)
    inode(9, 0x81A4, len(OSREL), 1, B_OSREL)
    inode(10, 0x41ED, BLK, 2, B_ETCDEF)       # /etc/default
    inode(11, 0x81A4, len(GRUB_DEFAULT), 1, B_DEFGRUB)
    wblock(part_lba + B_ITABLE * 2, itable)

    def dent(ino, name, rec_len, ftype):
        e = bytearray(rec_len)
        w32(e, 0, ino)
        w16(e, 4, rec_len)
        e[6] = len(name)
        e[7] = ftype
        e[8:8 + len(name)] = name
        return e

    def dir_block(entries):
        blk = bytearray(BLK)
        o = 0
        for ino, name, ftype in entries:
            rec = (8 + len(name) + 3) & ~3
            blk[o:o + rec] = dent(ino, name, rec, ftype)
            o += rec
        w32(blk, o, 0)
        w16(blk, o + 4, BLK - o)  # inode-0 entry spanning the remainder
        return blk

    wblock(part_lba + B_ROOT * 2, dir_block([
        (2, b".", 2), (2, b"..", 2), (3, b"etc", 2), (5, b"hello.txt", 1), (6, b"boot", 2)]))
    wblock(part_lba + B_ETC * 2, dir_block([
        (3, b".", 2), (2, b"..", 2), (4, b"fstab", 1), (9, b"os-release", 1), (10, b"default", 2)]))
    wblock(part_lba + B_BOOT * 2, dir_block([
        (6, b".", 2), (2, b"..", 2),
        (7, b"vmlinuz-6.6.0-fantuan", 1), (8, b"initrd.img-6.6.0-fantuan", 1)]))
    wblock(part_lba + B_ETCDEF * 2, dir_block([
        (10, b".", 2), (2, b"..", 2), (11, b"grub", 1)]))
    data = bytearray(BLK)
    data[0:len(fstab)] = fstab
    wblock(part_lba + B_fstab * 2, data)
    data = bytearray(BLK)
    data[0:len(HELLO_EXT4)] = HELLO_EXT4
    wblock(part_lba + B_HELLO * 2, data)
    data = bytearray(BLK)
    data[0:len(OSREL)] = OSREL
    wblock(part_lba + B_OSREL * 2, data)
    data = bytearray(BLK)
    data[0:len(GRUB_DEFAULT)] = GRUB_DEFAULT
    wblock(part_lba + B_DEFGRUB * 2, data)
    data = bytearray(BLK)
    data[0:len(VMLINUZ)] = VMLINUZ
    wblock(part_lba + B_VMLINUZ * 2, data)
    data = bytearray(BLK)
    data[0:len(INITRD)] = INITRD
    wblock(part_lba + B_INITRD * 2, data)
