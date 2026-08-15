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
fat[3 * 4:3 * 4 + 4] = b"\xFF\xFF\xFF\x0F"      # cluster 3 = EOC (HELLO.TXT)
fat[5 * 4:5 * 4 + 4] = struct.pack('<I', 6)          # 5 -> 6
fat[6 * 4:6 * 4 + 4] = b"\xFF\xFF\xFF\x0F"      # 6 = EOC (INFO.TXT chain)
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

root = bytearray(SECTOR)
root[0:32] = dent("HELLO", "TXT", 3, len(HELLO))
root[32:64] = dent("INFO", "TXT", 5, len(INFO))
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

path = sys.argv[1] if len(sys.argv) > 1 else "build/test.img"
with open(path, "wb") as f:
    f.write(out)
print(f"{path}: {len(out)} bytes, GPT + FAT32 ({CLUSTERS} clusters), HELLO.TXT + INFO.TXT")
