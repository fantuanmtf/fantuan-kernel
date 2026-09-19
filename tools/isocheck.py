#!/usr/bin/env python3
"""isocheck.py — structure checker for the M10-6 hybrid ISO written by
tools/mkiso.py (stdlib only). Validates the ISO9660 primary volume
descriptor, the El Torito catalog entries, the directory tree and the boot
image signatures. Split from mkiso.py to keep both files small.
"""

import struct

SECTOR = 2048
MAX_BYTES = 1 << 30
FAT_LABELS = (b"FAT12   ", b"FAT16   ", b"FAT32   ")


def _fail(msg):
    raise SystemExit(f"mkiso: check failed: {msg}")


def _parse_dir(img, lba, size):
    data = img[lba * SECTOR:lba * SECTOR + size]
    entries = {}
    off = 0
    while off < len(data):
        length = data[off]
        if length == 0:
            off = (off // SECTOR + 1) * SECTOR
            continue
        if length < 33 or off + length > len(data):
            _fail(f"bad directory record at LBA {lba}")
        rec = data[off:off + length]
        entries[bytes(rec[33:33 + rec[32]])] = (
            struct.unpack("<I", rec[2:6])[0],
            struct.unpack("<I", rec[10:14])[0],
            rec[25],
        )
        off += length
    return entries


def check(path):
    with open(path, "rb") as f:
        img = f.read()
    if len(img) % SECTOR:
        _fail("size is not a multiple of 2048")
    if len(img) > MAX_BYTES:
        _fail(f"{len(img)} bytes is over the 1 GiB budget")
    sectors = len(img) // SECTOR
    pvd = img[16 * SECTOR:17 * SECTOR]
    if pvd[0] != 1 or pvd[1:6] != b"CD001" or pvd[6] != 1:
        _fail("no primary volume descriptor at LBA 16")
    if struct.unpack("<I", pvd[80:84])[0] != sectors:
        _fail("PVD volume space size != file size")
    if struct.unpack("<H", pvd[128:130])[0] != SECTOR:
        _fail("PVD logical block size != 2048")
    root_lba = struct.unpack("<I", pvd[158:162])[0]
    root_size = struct.unpack("<I", pvd[166:170])[0]

    br = img[17 * SECTOR:18 * SECTOR]
    if br[0] != 0 or br[1:6] != b"CD001" or br[7:30] != b"EL TORITO SPECIFICATION":
        _fail("no El Torito boot record at LBA 17")
    cat_lba = struct.unpack("<I", br[71:75])[0]
    if img[18 * SECTOR] != 255:
        _fail("no volume descriptor set terminator at LBA 18")

    cat = img[cat_lba * SECTOR:(cat_lba + 1) * SECTOR]
    if cat[0] != 1 or cat[30:32] != b"\x55\xaa" \
       or sum(struct.unpack("<16H", cat[:32])) & 0xFFFF:
        _fail("bad El Torito validation entry")
    if cat[32] != 0x88 or cat[33] != 0 or cat[34:36] != b"\xc0\x07":
        _fail("BIOS entry is not bootable no-emulation at load segment 0x07C0")
    bios_512 = struct.unpack("<H", cat[38:40])[0]
    bios_lba = struct.unpack("<I", cat[40:44])[0]
    if cat[64] not in (0x90, 0x91) or cat[65] != 0xEF or cat[66:68] != b"\x01\x00":
        _fail("no UEFI platform 0xEF section header")
    if sum(struct.unpack("<16H", cat[64:96])) & 0xFFFF:
        _fail("bad UEFI section header checksum")
    if cat[96] != 0x88 or cat[97] != 0:
        _fail("UEFI entry is not bootable no-emulation")
    esp_512 = struct.unpack("<H", cat[102:104])[0]
    esp_lba = struct.unpack("<I", cat[104:108])[0]

    root = _parse_dir(img, root_lba, root_size)
    for name in (b"BIOS.IMG;1", b"KERNEL.BIN;1", b"EFI"):
        if name not in root:
            _fail(f"/{name.decode()} missing")
    efi = _parse_dir(img, root[b"EFI"][0], root[b"EFI"][1])
    if b"BOOT" not in efi:
        _fail("/EFI/BOOT missing")
    efboot = _parse_dir(img, efi[b"BOOT"][0], efi[b"BOOT"][1])
    for name in (b"BOOTX64.EFI;1", b"ESP.IMG;1"):
        if name not in efboot:
            _fail(f"/EFI/BOOT/{name.decode()} missing")
    if root[b"BIOS.IMG;1"][0] != bios_lba:
        _fail("catalog BIOS LBA does not match /BIOS.IMG")
    if efboot[b"ESP.IMG;1"][0] != esp_lba:
        _fail("catalog UEFI LBA does not match /EFI/BOOT/ESP.IMG")
    if bios_512 * 512 < root[b"BIOS.IMG;1"][1]:
        _fail("catalog BIOS sector count is short")
    if esp_512 * 512 < efboot[b"ESP.IMG;1"][1]:
        _fail("catalog UEFI sector count is short")

    bios = img[bios_lba * SECTOR:bios_lba * SECTOR + bios_512 * 512]
    if bios[510:512] != b"\x55\xaa":
        _fail("BIOS image has no MBR signature")
    esp = img[esp_lba * SECTOR:esp_lba * SECTOR + esp_512 * 512]
    if esp[510:512] != b"\x55\xaa" or esp[54:62] not in FAT_LABELS:
        _fail("ESP image is not a FAT boot sector")
    print(f"{path}: ISO9660 PVD + L/M path tables, El Torito BIOS 0x00 -> "
          f"/BIOS.IMG (LBA {bios_lba}), UEFI 0xEF -> /EFI/BOOT/ESP.IMG (LBA {esp_lba})")
    print(f"{path}: {len(img)} bytes ({len(img) / 1048576:.2f} MiB), budget 1 GiB: OK")
