#!/usr/bin/env python3
"""mkiso.py — write the M10-6 hybrid BIOS+UEFI ISO9660 image (stdlib only).

Layout decisions:
  * ISO9660 primary volume descriptor (LBA 16), an El Torito boot record
    (LBA 17), the set terminator, L/M path tables and a small tree:
      /BIOS.IMG              El Torito BIOS boot image (stage1+stage2+kernel)
      /KERNEL.BIN            flat x86_64 kernel (informational copy)
      /EFI/BOOT/BOOTX64.EFI  UEFI loader (same file as in the ESP image)
      /EFI/BOOT/ESP.IMG      FAT16 ESP image the UEFI entry points at
  * El Torito catalog: validation entry (platform 0x00), the BIOS default
    entry in no-emulation (media 0x00) with load segment 0x07C0. SeaBIOS
    refuses extended (AH=42h) reads on an emulated drive, so hard-disk
    emulation cannot serve stage1's LBA chain; no-emulation instead makes
    the firmware preload the whole boot image (stage1+stage2+kernel) at
    0x7C00, and stage2 copies the kernel from 0xC000 to its 16 MiB link
    address in protected mode. Then a final section header for platform
    0xEF and the UEFI no-emulation entry.
  * 1 GiB budget: build() refuses to go over it; --check IMAGE re-verifies
    an existing image (structure, El Torito entries, extents and signatures).

Usage:
  tools/mkiso.py --output OUT.iso --bios IMG --kernel BIN --bootx64 EFI --esp IMG
  tools/mkiso.py --check OUT.iso
"""

import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from isocheck import check  # noqa: E402

SECTOR = 2048
MAX_BYTES = 1 << 30
ISO_DATE = b"2026091900000000\x00"
REC_DATE = bytes([126, 9, 19, 0, 0, 0, 0])
FAT_LABELS = (b"FAT12   ", b"FAT16   ", b"FAT32   ")


def _both16(v):
    return struct.pack("<H", v) + struct.pack(">H", v)


def _both32(v):
    return struct.pack("<I", v) + struct.pack(">I", v)


def _rec(name, lba, size, flags):
    length = 33 + len(name)
    if length & 1:
        length += 1
    r = bytearray(length)
    r[0] = length
    r[2:10] = _both32(lba)
    r[10:18] = _both32(size)
    r[18:25] = REC_DATE
    r[25] = flags
    r[28:32] = _both16(1)
    r[32] = len(name)
    r[33:33 + len(name)] = name
    return bytes(r)


def _path_table(entries, big):
    out = bytearray()
    for name, lba, parent in entries:
        r = bytearray([len(name), 0])
        r += struct.pack(">I" if big else "<I", lba)
        r += struct.pack(">H" if big else "<H", parent)
        r += name
        if len(r) & 1:
            r.append(0)
        out += r
    return bytes(out)


def _checksum(block):
    return (-sum(struct.unpack("<16H", bytes(block[:32])))) & 0xFFFF


def _catalog(bios_lba, bios_512, esp_lba, esp_512):
    cat = bytearray(SECTOR)
    cat[0] = 1
    cat[30:32] = b"\x55\xaa"
    cat[28:30] = struct.pack("<H", _checksum(cat))
    cat[32] = 0x88
    cat[33] = 0x00
    cat[34:36] = b"\xc0\x07"
    cat[38:40] = struct.pack("<H", bios_512)
    cat[40:44] = struct.pack("<I", bios_lba)
    cat[64] = 0x91
    cat[65] = 0xEF
    cat[66:68] = struct.pack("<H", 1)
    cat[94:96] = b"\x55\xaa"
    cat[92:94] = struct.pack("<H", _checksum(cat[64:]))
    cat[96] = 0x88
    cat[97] = 0x00
    cat[102:104] = struct.pack("<H", esp_512)
    cat[104:108] = struct.pack("<I", esp_lba)
    return bytes(cat)


def build(bios, kernel, bootx64, esp):
    for label, data in (("bios", bios), ("kernel", kernel),
                        ("bootx64", bootx64), ("esp", esp)):
        if not data:
            raise SystemExit(f"mkiso: --{label} is empty")
    bios_512 = (len(bios) + 511) // 512
    esp_512 = (len(esp) + 511) // 512
    if bios_512 > 0xFFFF or esp_512 > 0xFFFF:
        raise SystemExit("mkiso: boot image too large for the El Torito sector count")

    lba = 16
    pvd_lba, lba = lba, lba + 1
    bootrec_lba, lba = lba, lba + 1
    term_lba, lba = lba, lba + 1
    lpath_lba, lba = lba, lba + 1
    mpath_lba, lba = lba, lba + 1
    root_lba, lba = lba, lba + 1
    efi_lba, lba = lba, lba + 1
    efboot_lba, lba = lba, lba + 1
    catalog_lba, lba = lba, lba + 1

    def place(data):
        nonlocal lba
        start = lba
        lba += (len(data) + SECTOR - 1) // SECTOR
        return start

    bios_lba = place(bios)
    kernel_lba = place(kernel)
    bootx64_lba = place(bootx64)
    esp_lba = place(esp)
    total = lba * SECTOR
    if total > MAX_BYTES:
        raise SystemExit(
            f"mkiso: image would be {total} bytes, over the 1 GiB budget "
            f"({MAX_BYTES}); shrink the ESP or the BIOS image")

    ptable = _path_table([(b"\x00", root_lba, 1), (b"EFI", efi_lba, 1),
                          (b"BOOT", efboot_lba, 2)], False)
    mtable = _path_table([(b"\x00", root_lba, 1), (b"EFI", efi_lba, 1),
                          (b"BOOT", efboot_lba, 2)], True)
    if len(ptable) > SECTOR:
        raise SystemExit("mkiso: path table does not fit one sector")

    root = _rec(b"\x00", root_lba, SECTOR, 2) + _rec(b"\x01", root_lba, SECTOR, 2)
    root += _rec(b"BIOS.IMG;1", bios_lba, len(bios), 0)
    root += _rec(b"EFI", efi_lba, SECTOR, 2)
    root += _rec(b"KERNEL.BIN;1", kernel_lba, len(kernel), 0)
    efi = _rec(b"\x00", efi_lba, SECTOR, 2) + _rec(b"\x01", root_lba, SECTOR, 2)
    efi += _rec(b"BOOT", efboot_lba, SECTOR, 2)
    efboot = _rec(b"\x00", efboot_lba, SECTOR, 2) + _rec(b"\x01", efi_lba, SECTOR, 2)
    efboot += _rec(b"BOOTX64.EFI;1", bootx64_lba, len(bootx64), 0)
    efboot += _rec(b"ESP.IMG;1", esp_lba, len(esp), 0)

    pvd = bytearray(SECTOR)
    pvd[0] = 1
    pvd[1:6] = b"CD001"
    pvd[6] = 1
    pvd[8:40] = b"FANTUAN".ljust(32)
    pvd[40:72] = b"FANTUAN_KERNEL".ljust(32)
    pvd[80:88] = _both32(total // SECTOR)
    pvd[120:124] = _both16(1)
    pvd[124:128] = _both16(1)
    pvd[128:132] = _both16(SECTOR)
    pvd[132:140] = _both32(len(ptable))
    pvd[140:144] = struct.pack("<I", lpath_lba)
    pvd[148:152] = struct.pack(">I", mpath_lba)
    pvd[156:190] = _rec(b"\x00", root_lba, SECTOR, 2)
    pvd[190:318] = b" "
    pvd[318:446] = b" "
    pvd[446:574] = b" "
    pvd[574:702] = b"FANTUAN M10-6".ljust(128)
    pvd[702:739] = b" "
    pvd[739:776] = b" "
    pvd[776:813] = b" "
    pvd[813:830] = ISO_DATE
    pvd[830:847] = ISO_DATE
    pvd[847:864] = b"0" * 16 + b"\x00"
    pvd[864:881] = b"0" * 16 + b"\x00"
    pvd[881] = 1

    bootrec = bytearray(SECTOR)
    bootrec[1:6] = b"CD001"
    bootrec[6] = 1
    bootrec[7:30] = b"EL TORITO SPECIFICATION"
    bootrec[71:75] = struct.pack("<I", catalog_lba)

    term = bytearray(SECTOR)
    term[0] = 255
    term[1:6] = b"CD001"
    term[6] = 1

    img = bytearray(total)

    def put(at, data):
        img[at * SECTOR:at * SECTOR + len(data)] = data

    put(pvd_lba, pvd)
    put(bootrec_lba, bootrec)
    put(term_lba, term)
    put(lpath_lba, ptable)
    put(mpath_lba, mtable)
    put(root_lba, root)
    put(efi_lba, efi)
    put(efboot_lba, efboot)
    put(catalog_lba, _catalog(bios_lba, bios_512, esp_lba, esp_512))
    put(bios_lba, bios)
    put(kernel_lba, kernel)
    put(bootx64_lba, bootx64)
    put(esp_lba, esp)
    return bytes(img)


def main(argv):
    if len(argv) == 3 and argv[1] == "--check":
        check(argv[2])
        return
    opts = {}
    for i, a in enumerate(argv[1:]):
        if a in ("--output", "--bios", "--kernel", "--bootx64", "--esp"):
            opts[a] = argv[i + 2]
    for key in ("--output", "--bios", "--kernel", "--bootx64", "--esp"):
        if key not in opts:
            raise SystemExit("usage: mkiso.py --output OUT --bios IMG --kernel BIN "
                             "--bootx64 EFI --esp IMG | --check OUT")
    with open(opts["--bios"], "rb") as f:
        bios = f.read()
    with open(opts["--kernel"], "rb") as f:
        kernel = f.read()
    with open(opts["--bootx64"], "rb") as f:
        bootx64 = f.read()
    with open(opts["--esp"], "rb") as f:
        esp = f.read()
    img = build(bios, kernel, bootx64, esp)
    with open(opts["--output"], "wb") as f:
        f.write(img)
    print(f"{opts['--output']}: {len(img)} bytes ({len(img) / 1048576:.2f} MiB), "
          f"budget 1 GiB: OK")


if __name__ == "__main__":
    main(sys.argv)
