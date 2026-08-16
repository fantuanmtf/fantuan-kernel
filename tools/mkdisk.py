#!/usr/bin/env python3
"""mkdisk.py — build the M6 test disk from scratch: a GPT disk with one
FAT32 partition containing two files:
  HELLO.TXT  single-cluster (37 bytes)
  INFO.TXT   two clusters (1000 bytes) — exercises the cluster-chain walk.
Zero host-tool dependencies (no sfdisk/mkfs.fat)."""

import struct
import sys
import zlib

SECTOR = 512
DISK_SECTORS = 32768  # 16 MiB
PART_LBA = 2048
PART_SECTORS = 30720  # 15 MiB

out = bytearray(SECTOR * DISK_SECTORS)

BROKEN = "--broken" in sys.argv or "--broken-shim" in sys.argv
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
hdr[88:92] = struct.pack('<I', zlib.crc32(entries) & 0xFFFFFFFF)
hdr[16:20] = struct.pack('<I', zlib.crc32(hdr) & 0xFFFFFFFF)

wsect(1, hdr)
out[2 * SECTOR:2 * SECTOR + len(entries)] = entries

# --- FAT32 partition -------------------------------------------------------
RESERVED = 32
N_FATS = 2
SPC = 1
SPF = 240
CLUSTERS = 30208
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
for n in range(7, 15):                                # 7..14 = ESP structure, all EOC
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
root[0:32] = dent("HELLO", "TXT", 3, len(HELLO))
root[32:64] = dent("INFO", "TXT", 5, len(INFO))
root[64:96] = dent("EFI", "   ", 7, 0, attrs=0x10)
root[96:128] = dent("FSTAB", "   ", 14, len(FSTAB))
wsect(cluster_sector(2), root)

hello = bytearray(SECTOR)
hello[0:len(HELLO)] = HELLO
wsect(cluster_sector(3), hello)

i0 = bytearray(SECTOR)
i0[0:512] = INFO[0:512]
wsect(cluster_sector(5), i0)
i1 = bytearray(SECTOR)
i1[0:488] = INFO[512:1000]
wsect(cluster_sector(6), i1)

# --- ESP structure (M7 boot-repair fixture) -------------------------------
# Cluster map: 7=EFI/ 8=EFI/BOOT/ 9=EFI/ubuntu/ 10=BOOTX64.EFI 11=grub.cfg
# 12=shimx64.efi 13=grubx64.efi 14=fstab

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

if not BROKEN:
    put_file(10, BOOTX64)
put_file(11, GRUBCFG)
if not NOSHIM:
    put_file(12, SHIM)
put_file(13, GRUBX64)
put_file(14, FSTAB)

args = [a for a in sys.argv[1:] if not a.startswith("--")]
path = args[0] if args else "build/test.img"
with open(path, "wb") as f:
    f.write(out)
print(f"{path}: {len(out)} bytes, GPT + FAT32 ({CLUSTERS} clusters), HELLO.TXT + INFO.TXT"
      + (" (broken ESP: no BOOTX64.EFI)" if BROKEN else "")
      + (" + no shim" if NOSHIM else ""))
