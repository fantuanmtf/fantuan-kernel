#!/usr/bin/env python3
"""mkntfs_fs.py — hand-built deterministic NTFS fixture encoder for
tools/mkntfs.py (M12-4). Low-level on-disk pieces only: update-sequence
fixups, record/attribute headers, runlists, $I30 index entries and the
boot sector. Split out of mkntfs.py to keep every file inside the size
rule; not a standalone tool. Validated against ntfs-3g (ntfsls/ntfscat).
"""

import struct

SECTOR = 512
CLUSTER = 4096
SPC = 8                    # sectors per cluster
REC = 1024                 # MFT record size
FT = 132944736000000000    # fixed FILETIME: 2022-05-15, deterministic

ATTR_STANDARD_INFO = 0x10
ATTR_FILE_NAME = 0x30
ATTR_VOLUME_NAME = 0x60
ATTR_VOLUME_INFO = 0x70
ATTR_DATA = 0x80
ATTR_INDEX_ROOT = 0x90
ATTR_INDEX_ALLOC = 0xA0
ATTR_BITMAP = 0xB0

ATTR_HIDDEN = 0x02
ATTR_SYSTEM = 0x04
ATTR_ARCHIVE = 0x20
ATTR_DIR = 0x10000000
META = ATTR_HIDDEN | ATTR_SYSTEM
I30 = "$I30"               # the standard directory index name


def enc_runs(runs):
    """Encode [(clusters, lcn or None)] as an NTFS runlist (None = sparse)."""
    out = bytearray()
    prev = 0
    for length, lcn in runs:
        lb = max(1, (length.bit_length() + 7) // 8)
        if lcn is None:
            out.append(lb)
            out += length.to_bytes(lb, 'little')
            continue
        delta = lcn - prev
        ob = max(1, (abs(delta).bit_length() + 8) // 8)
        out.append((ob << 4) | lb)
        out += length.to_bytes(lb, 'little')
        out += delta.to_bytes(ob, 'little', signed=True)
        prev = lcn
    out.append(0)
    return bytes(out)


def attr_res(ty, value, name="", aid=0):
    """One resident attribute (value inline in the MFT record)."""
    nb = len(name) * 2
    voff = (0x18 + nb + 7) & ~7
    length = (voff + len(value) + 7) & ~7
    a = bytearray(length)
    struct.pack_into('<IIBBHHH', a, 0, ty, length, 0, len(name), 0x18, 0, aid)
    struct.pack_into('<IHBB', a, 0x10, len(value), voff, 0, 0)
    if name:
        a[0x18:0x18 + nb] = name.encode('utf-16-le')
    a[voff:voff + len(value)] = value
    return bytes(a)


def attr_nres(ty, runs, alloc, data, init, name="", aid=0, lowest=0):
    """One non-resident attribute (the value is a runlist)."""
    run_off = (0x40 + len(name) * 2 + 7) & ~7
    length = (run_off + len(runs) + 7) & ~7
    a = bytearray(length)
    struct.pack_into('<IIBBHHH', a, 0, ty, length, 1, len(name), 0x40 if name else 0, 0, aid)
    highest = max(0, (alloc + CLUSTER - 1) // CLUSTER - 1)
    struct.pack_into('<QQHHI', a, 0x10, lowest, highest, run_off, 0, 0)
    struct.pack_into('<QQQ', a, 0x28, alloc, data, init)
    if name:
        a[0x40:0x40 + len(name) * 2] = name.encode('utf-16-le')
    a[run_off:run_off + len(runs)] = runs
    return bytes(a)


def standard_info(attrs):
    v = bytearray(0x48)
    for off in (0x00, 0x08, 0x10, 0x18):
        struct.pack_into('<Q', v, off, FT)
    struct.pack_into('<I', v, 0x20, attrs)
    return bytes(v)


def file_name(name, parent_ref, size, attrs):
    """A $FILE_NAME value, also the $I30 index key (namespace Win32)."""
    v = bytearray(0x42)
    struct.pack_into('<Q', v, 0, parent_ref)
    for off in (0x08, 0x10, 0x18, 0x20):
        struct.pack_into('<Q', v, off, FT)
    alloc = (size + CLUSTER - 1) // CLUSTER * CLUSTER
    struct.pack_into('<QQ', v, 0x28, alloc, size)
    struct.pack_into('<II', v, 0x38, attrs, 0)
    v[0x40] = len(name)
    v[0x41] = 1
    v += name.encode('utf-16-le')
    return bytes(v)


def index_entry(ref, key, flags=0, child=None):
    klen = len(key)
    length = (0x10 + klen + (8 if child is not None else 0) + 7) & ~7
    e = bytearray(length)
    struct.pack_into('<QHHHH', e, 0, ref, length, klen, flags, 0)
    e[0x10:0x10 + klen] = key
    if child is not None:
        struct.pack_into('<Q', e, 0x10 + klen, child)
    return bytes(e)


def index_end(child=None):
    return index_entry(0, b"", flags=0x2 | (1 if child is not None else 0), child=child)


def index_root(entries, large_child=None):
    """$INDEX_ROOT value: a small index, or a root pointing into $I30."""
    if large_child is not None:
        hdr = struct.pack('<IIII', 0x10, 0x28, 0x28, 1)
        ents = index_entry(0, b"", flags=0x3, child=large_child)
    else:
        hdr = struct.pack('<IIII', 0x10, 0x10 + len(entries), 0x10 + len(entries), 0)
        ents = entries
    return struct.pack('<IIIB', 0x30, 1, CLUSTER, 1) + bytes(3) + hdr + ents


def apply_fixups(rec, usa_off, usn=1):
    """Update-sequence protection: save each sector tail, store the USN."""
    n = len(rec) // SECTOR
    struct.pack_into('<H', rec, usa_off, usn)
    for s in range(n):
        off = (s + 1) * SECTOR - 2
        struct.pack_into('<H', rec, usa_off + 2 * (s + 1), struct.unpack_from('<H', rec, off)[0])
        struct.pack_into('<H', rec, off, usn)


def record(number, flags, attrs, seq=1, links=1):
    """One 1024-byte FILE record with its fixup applied."""
    rec = bytearray(REC)
    used = 0x38
    for a in attrs:
        rec[used:used + len(a)] = a
        used += len(a)
    struct.pack_into('<II', rec, used, 0xFFFFFFFF, 0)
    used += 8
    rec[0:4] = b'FILE'
    struct.pack_into('<HH', rec, 4, 0x30, 3)          # usa offset/count
    struct.pack_into('<HH', rec, 0x10, seq, links)
    struct.pack_into('<H', rec, 0x14, 0x38)           # first attribute
    struct.pack_into('<H', rec, 0x16, flags)
    struct.pack_into('<I', rec, 0x18, used)
    struct.pack_into('<I', rec, 0x1C, REC)
    struct.pack_into('<H', rec, 0x28, 0x20)           # next attribute id
    struct.pack_into('<I', rec, 0x2C, number)
    apply_fixups(rec, 0x30)
    return bytes(rec)


def indx(vcn, entries):
    """One 4096-byte INDX index-allocation block (leaf) with fixups."""
    blk = bytearray(CLUSTER)
    blk[0:4] = b'INDX'
    eo = 0x28
    struct.pack_into('<HH', blk, 4, 0x28, 1 + CLUSTER // SECTOR)
    struct.pack_into('<Q', blk, 0x10, vcn)
    struct.pack_into('<IIII', blk, 0x18, eo, eo + len(entries), CLUSTER - 0x18, 0)
    blk[0x18 + eo:0x18 + eo + len(entries)] = entries
    apply_fixups(blk, 0x28, usn=5)
    return bytes(blk)


def boot_sector(total_sectors, mft_lcn, mirr_lcn):
    v = bytearray(SECTOR)
    v[0:3] = b'\xEB\x52\x90'
    v[3:11] = b'NTFS    '
    struct.pack_into('<H', v, 0x0B, SECTOR)
    v[0x0D] = SPC
    v[0x15] = 0xF8
    struct.pack_into('<H', v, 0x18, 63)
    struct.pack_into('<H', v, 0x1A, 255)
    struct.pack_into('<Q', v, 0x28, total_sectors)
    struct.pack_into('<Q', v, 0x30, mft_lcn)
    struct.pack_into('<Q', v, 0x38, mirr_lcn)
    struct.pack_into('<b', v, 0x40, -10)     # 2^-(-10) = 1024-byte records
    struct.pack_into('<b', v, 0x44, 1)       # one-cluster index blocks
    struct.pack_into('<Q', v, 0x48, 0x123456789ABCDEF0)
    v[510:512] = b'\x55\xAA'
    return bytes(v)


def upcase_table():
    """A deterministic default $UpCase (ASCII + Latin-1 upper mapping)."""
    t = bytearray(0x20000)
    for i in range(0x10000):
        c = i
        if 0x61 <= i <= 0x7A:
            c = i - 0x20
        elif 0xE0 <= i <= 0xFE and i != 0xF7:
            c = i - 0x20
        struct.pack_into('<H', t, 2 * i, c)
    return bytes(t)
