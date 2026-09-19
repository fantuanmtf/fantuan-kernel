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
  --grub-regen               autorun runs `grub-fix install` (implies --two-fs)
Zero host-tool dependencies (no sfdisk/mkfs.fat). The FAT32 fixture lives in
mkdisk_fat.py and the ext4 root in mkdisk_ext4.py (file-size rule)."""

import os
import struct
import sys
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mkdisk_ext4 import build as build_ext4  # noqa: E402
from mkdisk_fat import build as build_fat  # noqa: E402

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
# --kbd-test: minimal ESP script so the monitor-injected keystrokes are
# consumed quickly (M8.5a smoke phase).
KBD_TEST = "--kbd-test" in sys.argv
# --two-fs adds a hand-built ext4 root (mounted ro by M6.5) and an XFS-magic
# stub that must stay probe-only.
# UEFI's FAT driver refuses a FAT32 volume below 0xFFF5 clusters (EDK2
# FatPkg/EnhancedFatDxe/Init.c), so the ESP partition is sized for 65614
# one-sector clusters (SPF 521). The disk keeps BACKUP_GPT_SECTORS after the
# last partition for the backup GPT the spec requires.
BACKUP_GPT_SECTORS = 33  # 32 partition-entry sectors + the header sector
PART_LBA = 2048
PART_SECTORS = 66688  # 32.6 MiB ESP
PART2_LBA = 69632
PART2_SECTORS = 8192  # 4 MiB (ext4 root fixture)
PART3_LBA = 77824
PART3_SECTORS = 8192  # 4 MiB (XFS-magic probe fixture)
_LAST_PART_END = PART3_LBA + PART3_SECTORS if TWO_FS else PART_LBA + PART_SECTORS
DISK_SECTORS = _LAST_PART_END + BACKUP_GPT_SECTORS + 1  # 42 / 33.6 MiB

out = bytearray(SECTOR * DISK_SECTORS)

BROKEN = "--broken" in sys.argv or "--broken-shim" in sys.argv
KEYS = "--keys" in sys.argv or "--shell-repair" in sys.argv or GRUB_REGEN or KBD_TEST
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
# The header CRC covers exactly HeaderSize (92) bytes, not the whole sector.
# UEFI validates that range and rejects the disk when it does not match.
hdr[88:92] = struct.pack('<I', zlib.crc32(entries) & 0xFFFFFFFF)
hdr[16:20] = struct.pack('<I', zlib.crc32(bytes(hdr[:92])) & 0xFFFFFFFF)

wsect(1, hdr)
out[2 * SECTOR:2 * SECTOR + len(entries)] = entries

# --- GPT: backup header + entries at the end of the disk -------------------
LAST_LBA = DISK_SECTORS - 1
bkp_entries_lba = LAST_LBA - 32  # 32 entry sectors immediately before the header
out[bkp_entries_lba * SECTOR:bkp_entries_lba * SECTOR + len(entries)] = entries
bkp = bytearray(SECTOR)
bkp[0:8] = b"EFI PART"
bkp[8:12] = struct.pack('<I', 0x00010000)
bkp[12:16] = struct.pack('<I', 92)
bkp[24:32] = struct.pack('<Q', LAST_LBA)                  # current LBA
bkp[32:40] = struct.pack('<Q', 1)                         # primary LBA
bkp[40:48] = struct.pack('<Q', 34)                        # first usable
bkp[48:56] = struct.pack('<Q', DISK_SECTORS - 34)         # last usable
bkp[56:72] = b"FANTUANDISKGUID!"                          # disk GUID
bkp[72:80] = struct.pack('<Q', bkp_entries_lba)           # entries LBA
bkp[80:84] = struct.pack('<I', 32)
bkp[84:88] = struct.pack('<I', 128)
bkp[88:92] = struct.pack('<I', zlib.crc32(entries) & 0xFFFFFFFF)
bkp[16:20] = struct.pack('<I', zlib.crc32(bytes(bkp[:92])) & 0xFFFFFFFF)
wsect(LAST_LBA, bkp)

# M7 boot-repair fixture: the PARTUUID in fstab must equal the GPT unique
# GUID of the FAT32 partition, in the text form Linux uses. Shared by the
# FAT32 fixture (ESP copy) and the ext4 root (/etc/fstab).
_ug = b"FANTUANPART0001!"
_PARTUUID = ("%08x-%04x-%04x-%s-%s") % (
    struct.unpack('<I', _ug[0:4])[0],
    struct.unpack('<H', _ug[4:6])[0],
    struct.unpack('<H', _ug[6:8])[0],
    _ug[8:12].hex(),
    _ug[12:16].hex(),
)
FSTAB = (
    b"UUID=12345678-1234-1234-1234-123456789abc / ext4 errors=remount-ro 0 1\n"
    + ("PARTUUID=%s /boot/efi vfat umask=0077 0 1\n" % _PARTUUID).encode()
)

flags = {
    "broken": BROKEN,
    "noshim": NOSHIM,
    "keys": KEYS,
    "two_fs": TWO_FS,
    "bigcluster": BIGCLUSTER,
    "liar": LIAR,
    "shell_repair": SHELL_REPAIR,
    "grub_regen": GRUB_REGEN,
    "kbd_test": KBD_TEST,
}
SPC, SPF, CLUSTERS = build_fat(out, PART_LBA, PART_SECTORS, flags, FSTAB)

if TWO_FS:
    # A real ext4 root on part 2 (mounted ro by the M6.5 driver) and XFS
    # magic on part 3 (must stay probe-only).
    build_ext4(out, PART2_LBA, FSTAB)
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
