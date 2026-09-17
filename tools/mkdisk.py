#!/usr/bin/env python3
"""mkdisk.py — build the test disk from scratch: a GPT disk with a FAT32 ESP
fixture (HELLO.TXT/INFO.TXT, EFI/BOOT/EFI/ubuntu trees, fstab/grub.cfg, NVRAM
and Secure Boot fixtures) and optional variant files:
  --broken / --broken-shim   missing fallback loader / shim
  --two-fs                   real ext4 root (mounted ro by M6.5) + XFS-magic
                             probe stub on the third partition
  --keys / --shell-repair    Secure Boot certs + ESP autorun shell script
  --liar / --bigcluster      crafted-input fixtures: oversized FAT entry,
                             4 KiB clusters with a short final write
Zero host-tool dependencies (no sfdisk/mkfs.fat)."""

import struct
import sys
import zlib

SECTOR = 512
TWO_FS = "--two-fs" in sys.argv
# Audit fixtures:
#   --bigcluster  SPC=8 (4 KiB clusters): exercises the short-data cluster
#                 write (a 21-byte copy must not index past the chunk).
#   --liar        HELLO.TXT claims 4096 bytes while holding 35: the reader
#                 must truncate to the caller's buffer instead of panicking.
BIGCLUSTER = "--bigcluster" in sys.argv
LIAR = "--liar" in sys.argv
# --grub-regen runs `grub-fix install` from the ESP script (implies the ext4
# root so there is something to generate from).
GRUB_REGEN = "--grub-regen" in sys.argv
if GRUB_REGEN:
    TWO_FS = True
# --two-fs adds a hand-built ext4 root (mounted ro by M6.5) and an XFS-magic
# stub that must stay probe-only.
DISK_SECTORS = 51200 if TWO_FS else 32768  # 25 / 16 MiB
PART_LBA = 2048
PART_SECTORS = 30720  # 15 MiB
PART2_LBA = 32768
PART2_SECTORS = 8192  # 4 MiB (ext4 root fixture)
PART3_LBA = 40960
PART3_SECTORS = 8192  # 4 MiB (XFS-magic probe fixture)

out = bytearray(SECTOR * DISK_SECTORS)

BROKEN = "--broken" in sys.argv or "--broken-shim" in sys.argv
KEYS = "--keys" in sys.argv or "--shell-repair" in sys.argv or GRUB_REGEN
SHELL_REPAIR = "--shell-repair" in sys.argv
# --broken-shim: the fallback loader AND the shim are gone, so the fallback
# copy repair has nothing to copy from (exercises the NVRAM delete path).
NOSHIM = "--broken-shim" in sys.argv


def wsect(lba, data):
    out[lba * SECTOR:(lba + 1) * SECTOR] = data


# --- GPT: protective MBR ------------------------------------------------
out[510] = 0x55
out[511] = 0xAA
out[446 + 4] = 0xEE  # partition type: protective
out[446 + 8:446 + 12] = struct.pack('<I', 1)
out[446 + 12:446 + 16] = struct.pack('<I', DISK_SECTORS - 1)

# --- GPT: header (LBA 1) --------------------------------------------------
hdr = bytearray(SECTOR)
hdr[0:8] = b"EFI PART"
hdr[8:12] = struct.pack('<I', 0x00010000)
hdr[12:16] = struct.pack('<I', 92)
hdr[24:32] = struct.pack('<Q', 1)                      # current LBA
hdr[32:40] = struct.pack('<Q', DISK_SECTORS - 1)       # backup LBA
hdr[40:48] = struct.pack('<Q', 34)                     # first usable
hdr[48:56] = struct.pack('<Q', DISK_SECTORS - 34)      # last usable
hdr[56:72] = b"FANTUANDISKGUID!"                       # disk GUID
hdr[72:80] = struct.pack('<Q', 2)                      # entries LBA
hdr[80:84] = struct.pack('<I', 32)
hdr[84:88] = struct.pack('<I', 128)

# --- GPT: one FAT32 partition entry ---------------------------------------
ent = bytearray(128)
# type GUID: EBD0A0A2-B9E5-4433-87C0-68B6B72699C7 (mixed-endian on disk)
ent[0:16] = bytes.fromhex("a2a0d0ebe5b9334487c068b6b72699c7")
ent[16:32] = b"FANTUANPART0001!"                        # unique GUID
ent[32:40] = struct.pack('<Q', PART_LBA)
ent[40:48] = struct.pack('<Q', PART_LBA + PART_SECTORS - 1)
name = "FANTUAN ESP".encode("utf-16-le")
ent[56:56 + len(name)] = name

entries = bytearray(32 * 128)
entries[0:128] = ent

if TWO_FS:
    # Second partition: a real (minimal) ext4 root — the M6.5 reader must
    # mount it ro. Third partition: XFS magic — still probe-only.
    ent2 = bytearray(128)
    ent2[0:16] = bytes.fromhex("af3dc60f838472478e793d69d8477de4")  # Linux FS
    ent2[16:32] = b"FANTUANPART0002!"
    ent2[32:40] = struct.pack('<Q', PART2_LBA)
    ent2[40:48] = struct.pack('<Q', PART2_LBA + PART2_SECTORS - 1)
    name2 = "FANTUAN root".encode("utf-16-le")
    ent2[56:56 + len(name2)] = name2
    entries[128:256] = ent2

    ent3 = bytearray(128)
    ent3[0:16] = bytes.fromhex("af3dc60f838472478e793d69d8477de4")
    ent3[16:32] = b"FANTUANPART0003!"
    ent3[32:40] = struct.pack('<Q', PART3_LBA)
    ent3[40:48] = struct.pack('<Q', PART3_LBA + PART3_SECTORS - 1)
    name3 = "FANTUAN xfs".encode("utf-16-le")
    ent3[56:56 + len(name3)] = name3
    entries[256:384] = ent3
hdr[88:92] = struct.pack('<I', zlib.crc32(entries) & 0xFFFFFFFF)
hdr[16:20] = struct.pack('<I', zlib.crc32(hdr) & 0xFFFFFFFF)

wsect(1, hdr)
out[2 * SECTOR:2 * SECTOR + len(entries)] = entries

# --- FAT32 partition -------------------------------------------------------
RESERVED = 32
N_FATS = 2
# --bigcluster uses 4 KiB clusters; the FAT is sized to cover the data area.
SPC = 8 if BIGCLUSTER else 1
SPF = 30 if BIGCLUSTER else 240
CLUSTERS = (PART_SECTORS - RESERVED - N_FATS * SPF) // SPC
DATA_START = PART_LBA + RESERVED + N_FATS * SPF

bpb = bytearray(SECTOR)
bpb[0:3] = b"\xEB\x58\x90"
bpb[3:11] = b"FANTUAN1"
bpb[11:13] = struct.pack('<H', SECTOR)
bpb[13] = SPC
bpb[14:16] = struct.pack('<H', RESERVED)
bpb[16] = N_FATS
bpb[19:21] = struct.pack('<H', 0)          # root entries (FAT32: 0)
bpb[21] = 0xF8
bpb[28:32] = struct.pack('<I', 0)          # hidden sectors
bpb[32:36] = struct.pack('<I', PART_SECTORS)
bpb[36:40] = struct.pack('<I', SPF)
bpb[44:48] = struct.pack('<I', 2)          # root cluster
bpb[0x52:0x5A] = b"FAT32   "               # filesystem type string
bpb[48:50] = struct.pack('<H', 1)          # FSInfo sector
bpb[50:52] = struct.pack('<H', 6)          # backup boot sector
bpb[64] = 0x80
bpb[66] = 0x29
bpb[510:512] = b"\x55\xAA"
wsect(PART_LBA, bpb)

# FATs (identical copies)
fat = bytearray(SPF * SECTOR)
fat[0:4] = b"\xF8\xFF\xFF\x0F"
fat[4:8] = b"\xFF\xFF\xFF\x0F"
fat[2 * 4:2 * 4 + 4] = b"\xFF\xFF\xFF\x0F"      # cluster 2 = EOC (root dir)
fat[3 * 4:3 * 4 + 4] = b"\xFF\xFF\xFF\x0F"      # cluster 3 = EOC (HELLO.TXT)
fat[5 * 4:5 * 4 + 4] = struct.pack('<I', 6)          # 5 -> 6
fat[6 * 4:6 * 4 + 4] = b"\xFF\xFF\xFF\x0F"      # 6 = EOC (INFO.TXT chain)
for n in range(7, 25):                                # 7..24 = ESP/systemd/UKI structure, all EOC
    fat[n * 4:n * 4 + 4] = b"\xFF\xFF\xFF\x0F"
wsect(PART_LBA + RESERVED, fat)
wsect(PART_LBA + RESERVED + SPF, fat)


def cluster_sector(n):
    return DATA_START + (n - 2) * SPC


def dent(name8, ext3, cluster, size, attrs=0x20):
    e = bytearray(32)
    e[0:8] = name8.ljust(8).encode()
    e[8:11] = ext3.ljust(3).encode()
    e[11] = attrs
    e[20:22] = struct.pack('<H', (cluster >> 16) & 0xFFFF)
    e[26:28] = struct.pack('<H', cluster & 0xFFFF)
    e[28:32] = struct.pack('<I', size)
    return e


HELLO = b"Hello from the fantuan-kernel VFS!\n"
INFO = b"X" * 1000
HELLO_EXT4 = b"hello from the ext4 root\n"
# M7.9 ext4 root fixture: /boot + os-release + default/grub feed the repair
# inventory and the grub.cfg generator.
OSREL = b'ID=fantuan\nVERSION_ID="6.6"\nPRETTY_NAME="Fantuan Test Linux"\n'
GRUB_DEFAULT = b'GRUB_DEFAULT=0\nGRUB_CMDLINE_LINUX="quiet splash"\n'
VMLINUZ = b"FANTUAN TEST KERNEL IMAGE\n" * 4
INITRD = b"FANTUAN TEST INITRD IMAGE\n" * 4
FANTUAN_CONF = b"title Fantuan test entry\nlinux /boot/vmlinuz-6.6.0-fantuan\n"

# M7 boot-repair fixture: the PARTUUID in fstab must equal the GPT unique
# GUID of the FAT32 partition, in the text form Linux uses.
_ug = b"FANTUANPART0001!"
_PARTUUID = ("%08x-%04x-%04x-%s-%s") % (
    struct.unpack('<I', _ug[0:4])[0],
    struct.unpack('<H', _ug[4:6])[0],
    struct.unpack('<H', _ug[6:8])[0],
    _ug[8:12].hex(),
    _ug[12:16].hex(),
)
GRUBCFG = (
    b"search.fs_uuid 12345678-1234-1234-1234-123456789abc root\n"
    b"set prefix=($root)'/boot/grub'\n"
    b"set root='hd0,gpt1'\n"
)
FSTAB = (
    b"UUID=12345678-1234-1234-1234-123456789abc / ext4 errors=remount-ro 0 1\n"
    + ("PARTUUID=%s /boot/efi vfat umask=0077 0 1\n" % _PARTUUID).encode()
)
BOOTX64 = b"FANTUAN FALLBACK EFI APP (dummy)\n"
SHIM = b"FANTUAN SHIM (dummy)\n"
GRUBX64 = b"FANTUAN GRUB (dummy)\n"

root = bytearray(SECTOR)
HELLO_SIZE = 4096 if LIAR else len(HELLO)
root[0:32] = dent("HELLO", "TXT", 3, HELLO_SIZE)
root[32:64] = dent("INFO", "TXT", 5, len(INFO))
root[64:96] = dent("EFI", "   ", 7, 0, attrs=0x10)
root[96:128] = dent("FSTAB", "   ", 14, len(FSTAB))
if TWO_FS:
    # systemd-boot config lives at the ESP root (M7.9 detection fixture).
    root[128:160] = dent("loader", "   ", 22, 0, attrs=0x10)
wsect(cluster_sector(2), root)

hello = bytearray(SECTOR)
hello[0:len(HELLO)] = HELLO
wsect(cluster_sector(3), hello)

i0 = bytearray(SECTOR)
i0[0:512] = INFO[0:512]
wsect(cluster_sector(5), i0)
i1 = bytearray(SECTOR)
i1[0:488] = INFO[512:1000]
if SPC == 1:
    # One sector per cluster: the second half lives in the next cluster.
    wsect(cluster_sector(6), i1)
else:
    # Both halves fit the same cluster (the 5->6 chain stays unused).
    wsect(cluster_sector(5) + 1, i1)

# --- ESP structure (M7 boot-repair fixture) -------------------------------
# Cluster map: 7=EFI/ 8=EFI/BOOT/ 9=EFI/ubuntu/ 10=BOOTX64.EFI 11=grub.cfg
# 12=shimx64.efi 13=grubx64.efi 14=fstab
# --keys adds 15=EFI/fantuan/ 16=PK.cer 17=KEK.cer 18=db.cer 19=SHELL.CMD
# (M7.7 keys + §10 shell autorun script)

def dotdot(parent):
    return dent(".", "   ", parent if parent else 0, 0, attrs=0x10)


def self_entry(cluster):
    return dent(".", "   ", cluster, 0, attrs=0x10)


def put_file(cluster, data):
    sec = bytearray(SECTOR)
    sec[0:len(data)] = data
    wsect(cluster_sector(cluster), sec)


EFI_DIR = bytearray(SECTOR)
EFI_DIR[0:32] = self_entry(7)
EFI_DIR[32:64] = dotdot(0)
EFI_DIR[64:96] = dent("BOOT", "   ", 8, 0, attrs=0x10)
EFI_DIR[96:128] = dent("ubuntu", "   ", 9, 0, attrs=0x10)
if KEYS:
    EFI_DIR[128:160] = dent("fantuan", "   ", 15, 0, attrs=0x10)
if TWO_FS:
    # EFI-stub / UKI fixture (M7.9): EFI/Linux/ holds bootable EFI images.
    off = 160 if KEYS else 128
    EFI_DIR[off:off + 32] = dent("Linux", "   ", 20, 0, attrs=0x10)
wsect(cluster_sector(7), EFI_DIR)

BOOT_DIR = bytearray(SECTOR)
BOOT_DIR[0:32] = self_entry(8)
BOOT_DIR[32:64] = dotdot(7)
if BROKEN:
    BOOT_DIR[64] = 0xE5  # deleted: simulate a missing fallback loader
else:
    BOOT_DIR[64:96] = dent("BOOTX64", "EFI", 10, len(BOOTX64))
wsect(cluster_sector(8), BOOT_DIR)

UBUNTU_DIR = bytearray(SECTOR)
UBUNTU_DIR[0:32] = self_entry(9)
UBUNTU_DIR[32:64] = dotdot(7)
UBUNTU_DIR[64:96] = dent("GRUB", "CFG", 11, len(GRUBCFG))
if NOSHIM:
    UBUNTU_DIR[96] = 0xE5  # deleted: simulate a missing shim
else:
    UBUNTU_DIR[96:128] = dent("SHIMX64", "EFI", 12, len(SHIM))
UBUNTU_DIR[128:160] = dent("GRUBX64", "EFI", 13, len(GRUBX64))
wsect(cluster_sector(9), UBUNTU_DIR)

# M7.7: platform-key fixtures for the Setup-Mode enrollment path. The blob is
# a dummy DER-ish certificate — Setup Mode accepts it unauthenticated.
CERT = b"\x30\x82\x00\x40" + (b"FANTUAN TEST CERTIFICATE " * 4)
# §10 shell autorun script. --shell-repair swaps in the confirmation-gated
# repair sequence (the shell feeds the next script line as the YES answer).
if GRUB_REGEN:
    SHELL_CMD = (
        b"grub-fix install\n"
        b"YES\n"
        b"cat /EFI/ubuntu/grub.cfg\n"
    )
elif SHELL_REPAIR:
    SHELL_CMD = (
        b"grub-fix repair\n"
        b"YES\n"
        b"cat /EFI/BOOT/BOOTX64.EFI\n"
    )
else:
    cmds = [
        b"help",
        b"lsdev",
        b"lsos",
        b"lsmnt",
        b"cat /HELLO.TXT",
        b"bootinfo",
        b"diskhealth",
    ]
    if TWO_FS:
        # Ext4 phase: keep the script fast and deterministic (the surface scan
        # would eat the phase budget on the 25 MiB disk); the scan itself is
        # covered by the default --keys phase.
        cmds.append(b"cat /etc/fstab")
    else:
        cmds.append(b"diskhealth --scan")
    SHELL_CMD = b"".join(c + b"\n" for c in cmds)
if KEYS:
    FANTUAN_DIR = bytearray(SECTOR)
    FANTUAN_DIR[0:32] = self_entry(15)
    FANTUAN_DIR[32:64] = dotdot(7)
    FANTUAN_DIR[64:96] = dent("PK", "CER", 16, len(CERT))
    FANTUAN_DIR[96:128] = dent("KEK", "CER", 17, len(CERT))
    FANTUAN_DIR[128:160] = dent("DB", "CER", 18, len(CERT))
    FANTUAN_DIR[160:192] = dent("SHELL", "CMD", 19, len(SHELL_CMD))
    wsect(cluster_sector(15), FANTUAN_DIR)
    put_file(16, CERT)
    put_file(17, CERT)
    put_file(18, CERT)
    put_file(19, SHELL_CMD)

if not BROKEN:
    put_file(10, BOOTX64)
put_file(11, GRUBCFG)
if not NOSHIM:
    put_file(12, SHIM)
put_file(13, GRUBX64)
put_file(14, FSTAB)

if TWO_FS:
    # systemd-boot config tree (M7.9): /loader/entries/FANTUAN.CON stands in
    # for a *.conf entry's 8.3 alias; EFI/Linux/FANTUAN.EFI is a UKI stub.
    LOADER_DIR = bytearray(SECTOR)
    LOADER_DIR[0:32] = self_entry(22)
    LOADER_DIR[32:64] = dotdot(0)
    LOADER_DIR[64:96] = dent("entries", "   ", 23, 0, attrs=0x10)
    wsect(cluster_sector(22), LOADER_DIR)
    ENTRIES_DIR = bytearray(SECTOR)
    ENTRIES_DIR[0:32] = self_entry(23)
    ENTRIES_DIR[32:64] = dotdot(22)
    ENTRIES_DIR[64:96] = dent("FANTUAN", "CON", 24, len(FANTUAN_CONF))
    wsect(cluster_sector(23), ENTRIES_DIR)
    put_file(24, FANTUAN_CONF)
    LINUX_DIR = bytearray(SECTOR)
    LINUX_DIR[0:32] = self_entry(20)
    LINUX_DIR[32:64] = dotdot(7)
    LINUX_DIR[64:96] = dent("FANTUAN", "EFI", 21, len(BOOTX64))
    wsect(cluster_sector(20), LINUX_DIR)
    put_file(21, BOOTX64)

def wblock(lba, data):
    """Write a multi-sector block at LBA (wsect is 512 bytes only)."""
    out[lba * SECTOR:lba * SECTOR + len(data)] = data


def build_ext4(part_lba):
    """Hand-built minimal ext4: 1 KiB blocks, one group, extent trees on every
    inode, root with /etc/fstab, /etc/os-release, /etc/default/grub,
    /hello.txt and /boot/vmlinuz+initrd (the M7.9 repair inventory). No
    journal, no checksums, no backup superblocks — just enough for the
    read-only rescue driver."""
    BLK = 1024
    N_BLOCKS = 64
    N_INODES = 16
    # Block plan (blocks are 1 KiB, i.e. 2 sectors).
    (B_SB, B_GDT, B_BBITMAP, B_IBITMAP, B_ITABLE, B_ROOT, B_ETC, B_FSTAB,
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
    inode(4, 0x81A4, len(FSTAB), 1, B_FSTAB)
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
    data[0:len(FSTAB)] = FSTAB
    wblock(part_lba + B_FSTAB * 2, data)
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


if TWO_FS:
    # A real ext4 root on part 2 (mounted ro by the M6.5 driver) and XFS
    # magic on part 3 (must stay probe-only).
    build_ext4(PART2_LBA)
    out[PART3_LBA * SECTOR:PART3_LBA * SECTOR + 4] = b"XFSB"

args = [a for a in sys.argv[1:] if not a.startswith("--")]
path = args[0] if args else "build/test.img"
with open(path, "wb") as f:
    f.write(out)
print(f"{path}: {len(out)} bytes, GPT + FAT32 ({CLUSTERS} clusters, SPC {SPC}), HELLO.TXT + INFO.TXT"
      + (" (broken ESP: no BOOTX64.EFI)" if BROKEN else "")
      + (" + no shim" if NOSHIM else "")
      + (" + 4 KiB clusters" if BIGCLUSTER else "")
      + (" + HELLO.TXT size lie" if LIAR else "")
      + (" + ext4 root + XFS probe fixtures" if TWO_FS else ""))
