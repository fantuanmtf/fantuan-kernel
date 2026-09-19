#!/usr/bin/env python3
r"""mkesp.py — build the FAT16 EFI System Partition image for the M10-6
hybrid ISO (stdlib only, no mkfs.vfat).

The El Torito UEFI entry points the firmware at this image; OVMF's FAT driver
binds to it, \EFI\BOOT\BOOTX64.EFI boots, and the loader reads
\fantuan\kernel.bin from the same volume. All names are 8.3 (the same names
the loader asks for); lower-case names get the NT case flags.

Usage: tools/mkesp.py [--size 4M] OUT.IMG NAME=PATH [NAME=PATH ...]
"""

import struct
import sys

SECTOR = 512
RESERVED = 1
N_FATS = 2
ROOT_ENTRIES = 512
ATTR_DIR = 0x10
ATTR_FILE = 0x20
EOC = 0xFFFF


def _parse_size(text):
    scale = 1
    if text[-1:].upper() == "M":
        scale, text = 1024 * 1024, text[:-1]
    elif text[-1:].upper() == "K":
        scale, text = 1024, text[:-1]
    return int(text) * scale


def _name11(name):
    if name in (".", ".."):
        return name.encode().ljust(11, b" "), 0
    base, dot, ext = name.partition(".")
    if not base or len(base) > 8 or len(ext) > 3 or "." in ext:
        raise SystemExit(f"mkesp: not an 8.3 name: {name!r}")
    if any(c in ' /\\:*?"<>|' for c in base + ext):
        raise SystemExit(f"mkesp: bad character in {name!r}")
    raw = (base.ljust(8) + ext.ljust(3)).upper().encode("ascii")
    flags = 0
    if base == base.lower():
        flags |= 0x08
    if ext and ext == ext.lower():
        flags |= 0x10
    return raw, flags


def _entry(name, attr, cluster, size):
    raw, flags = _name11(name)
    e = bytearray(32)
    e[0:11] = raw
    e[11] = attr
    e[12] = flags
    e[20:22] = struct.pack("<H", (cluster >> 16) & 0xFFFF)
    e[26:28] = struct.pack("<H", cluster & 0xFFFF)
    e[28:32] = struct.pack("<I", size)
    return bytes(e)


class Fat16:
    def __init__(self, size):
        if size % SECTOR or size < 1 << 20:
            raise SystemExit("mkesp: --size must be a 512-byte multiple >= 1 MiB")
        self.total = size // SECTOR
        self.root_sectors = ROOT_ENTRIES * 32 // SECTOR
        self.spf = -(-(self.total - RESERVED - self.root_sectors + 2) // (SECTOR // 2 + N_FATS))
        self.clusters = self.total - RESERVED - N_FATS * self.spf - self.root_sectors
        if not 4085 <= self.clusters <= 65524:
            raise SystemExit(
                f"mkesp: {size} bytes yields {self.clusters} clusters, not FAT16; "
                "pick another --size")
        self.data_start = RESERVED + N_FATS * self.spf + self.root_sectors
        self.fat = [0] * (self.clusters + 2)
        self.fat[0] = 0xFFF8
        self.fat[1] = 0xFFFF
        self.next = 2
        self.entries = {0: []}
        self.dirs = {"": 0}
        self.data = {}

    def _alloc(self):
        if self.next - 2 >= self.clusters:
            raise SystemExit("mkesp: image is full")
        c = self.next
        self.next += 1
        self.fat[c] = EOC
        return c

    def _find_dir(self, path):
        if path in self.dirs:
            return self.dirs[path]
        parent, _, name = path.rpartition("/")
        pc = self._find_dir(parent)
        c = self._alloc()
        self.dirs[path] = c
        self.entries[c] = [_entry(".", ATTR_DIR, c, 0), _entry("..", ATTR_DIR, pc, 0)]
        self.entries[pc].append(_entry(name, ATTR_DIR, c, 0))
        return c

    def add(self, path, data):
        parent, _, name = path.rpartition("/")
        pc = self._find_dir(parent)
        first = self._alloc()
        chain = [first]
        for i in range(0, len(data), SECTOR):
            if i:
                c = self._alloc()
                self.fat[chain[-1]] = c
                chain.append(c)
            self.data[chain[-1]] = data[i:i + SECTOR].ljust(SECTOR, b"\x00")
        self.entries[pc].append(_entry(name, ATTR_FILE, first, len(data)))

    def render(self):
        img = bytearray(self.total * SECTOR)
        bpb = bytearray(SECTOR)
        bpb[0:3] = b"\xeb\x3c\x90"
        bpb[3:11] = b"FANTUAN "
        bpb[11:13] = struct.pack("<H", SECTOR)
        bpb[13] = 1
        bpb[14:16] = struct.pack("<H", RESERVED)
        bpb[16] = N_FATS
        bpb[17:19] = struct.pack("<H", ROOT_ENTRIES)
        bpb[19:21] = struct.pack("<H", self.total if self.total < 0x10000 else 0)
        bpb[21] = 0xF8
        bpb[22:24] = struct.pack("<H", self.spf)
        bpb[24:26] = struct.pack("<H", 32)
        bpb[26:28] = struct.pack("<H", 64)
        bpb[32:36] = struct.pack("<I", self.total if self.total >= 0x10000 else 0)
        bpb[36] = 0x80
        bpb[38] = 0x29
        bpb[39:43] = struct.pack("<I", 0x4D313036)
        bpb[43:54] = b"FANTUAN ESP"
        bpb[54:62] = b"FAT16   "
        bpb[510:512] = b"\x55\xaa"
        img[0:SECTOR] = bpb

        fat = bytearray(self.spf * SECTOR)
        for i, v in enumerate(self.fat):
            struct.pack_into("<H", fat, i * 2, v)
        for n in range(N_FATS):
            off = (RESERVED + n * self.spf) * SECTOR
            img[off:off + len(fat)] = fat

        root_off = (RESERVED + N_FATS * self.spf) * SECTOR
        if len(self.entries[0]) * 32 > self.root_sectors * SECTOR:
            raise SystemExit("mkesp: root directory overflow")
        for i, e in enumerate(self.entries[0]):
            img[root_off + i * 32:root_off + i * 32 + 32] = e

        for c, data in self.data.items():
            off = (self.data_start + c - 2) * SECTOR
            img[off:off + SECTOR] = data
        for c, ents in self.entries.items():
            if c == 0:
                continue
            if len(ents) * 32 > SECTOR:
                raise SystemExit("mkesp: subdirectory does not fit one cluster")
            off = (self.data_start + c - 2) * SECTOR
            for i, e in enumerate(ents):
                img[off + i * 32:off + i * 32 + 32] = e
        return bytes(img)


def main(argv):
    size = 4 << 20
    rest = []
    it = iter(argv[1:])
    for a in it:
        if a == "--size":
            size = _parse_size(next(it))
        else:
            rest.append(a)
    if len(rest) < 2:
        raise SystemExit(__doc__)
    out, specs = rest[0], rest[1:]
    fs = Fat16(size)
    for spec in specs:
        name, sep, path = spec.partition("=")
        if not sep:
            raise SystemExit(f"mkesp: expected NAME=PATH, got {spec!r}")
        with open(path, "rb") as f:
            data = f.read()
        if not data:
            raise SystemExit(f"mkesp: {path} is empty")
        fs.add(name, data)
    img = fs.render()
    with open(out, "wb") as f:
        f.write(img)
    print(f"{out}: {len(img)} bytes, FAT16 ({fs.clusters} clusters, {fs.spf} sectors/FAT), "
          f"{len(specs)} file(s)")


if __name__ == "__main__":
    main(sys.argv)
