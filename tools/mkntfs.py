#!/usr/bin/env python3
"""mkntfs.py — hand-built deterministic NTFS fixture for tools/mkdisk.py
(M12-4/M12-5): a 16 MiB NTFS 3.1 volume with the $MFT and its first four
mirror records, $LogFile/$Volume/$AttrDef/$Bitmap/$Boot/$BadClus/$Secure/
$UpCase/$Extend, a root directory indexed through $I30 + an INDX block, a
small resident-index directory, resident files, a non-resident fragmented
file (three runs), a non-ASCII name and one deliberately corrupt FILE
record (bad update sequence) for the reject path.

The encoder lives in mkntfs_fs.py; this file lays out the volume, writes
the GPT entry and the ESP autorun transcript, and can extract the expected
file contents (`--extract DIR`) so a host smoke can hash them. No host
NTFS tool is used: the image is deterministic byte-for-byte and validated
against ntfs-3g (ntfsls/ntfscat). Not a standalone formatter."""

import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import mkntfs_fs as fs  # noqa: E402

NTFS_LBA = 86016             # first sector (1 MiB-aligned, after ESP/XFS)
NTFS_SECTORS = 32768         # 16 MiB = 2048 4 KiB clusters
NCLU = 2048
MFT_LCN = 4
MFT_CLUSTERS = 6
ROOT = 5
USERS = 18

HELLO = b"hello from the NTFS fixture\n"
RESUME = b"resume: non-ascii name test\n"
ALICE = b"alice from the fixture\n"
BOOTLOG = b"logs: boot fixture ok\n"
CORRUPT = b"corrupt record payload\n"


def frag_data():
    """The fragmented file: 10000 deterministic printable bytes."""
    data = b"".join(b"frag line %04d: %s\n" % (i, b"0123456789abcdef" * 2) for i in range(250))
    return data[:10000]


def files():
    return {
        "hello.txt": HELLO,
        "frag.bin": frag_data(),
        "r\u00e9sum\u00e9.txt": RESUME,
        "corrupt.txt": CORRUPT,
        "Users/alice.txt": ALICE,
        "Users/logs/boot.log": BOOTLOG,
    }


def gpt_entry(slot):
    """The GPT partition entry for the NTFS volume."""
    ent = bytearray(128)
    ent[0:16] = bytes.fromhex("a2a0d0ebe5b9334487c068b6b72699c7")  # basic data
    ent[16:32] = b"FANTUANNTFS00001!"
    struct.pack_into('<Q', ent, 32, NTFS_LBA)
    struct.pack_into('<Q', ent, 40, NTFS_LBA + NTFS_SECTORS - 1)
    name = "FANTUAN NTFS".encode('utf-16-le')
    ent[56:56 + len(name)] = name
    return bytes(ent)


def ref(n):
    return n | (1 << 48)


def install_ntfs(out, entries, slot):
    """Write the GPT entry and the NTFS volume into the mkdisk image."""
    entries[slot * 128:(slot + 1) * 128] = gpt_entry(slot)
    total = NTFS_SECTORS

    def wsect(lba, data):
        out[lba * fs.SECTOR:lba * fs.SECTOR + len(data)] = data

    def wdata(lcn, data):
        base = NTFS_LBA * fs.SECTOR + lcn * fs.CLUSTER
        out[base:base + len(data)] = data

    meta_names = [("$AttrDef", 4), ("$BadClus", 8), ("$Bitmap", 6), ("$Boot", 7),
                  ("$Extend", 11), ("$LogFile", 2), ("$MFT", 0), ("$MFTMirr", 1),
                  ("$Secure", 9), ("$UpCase", 10), ("$Volume", 3)]
    root_entries = b"".join(
        [fs.index_entry(ref(n), fs.file_name(nm, ref(ROOT), 0, fs.META)) for nm, n in meta_names]
        + [fs.index_entry(ref(ROOT), fs.file_name(".", ref(ROOT), 0, fs.ATTR_DIR)),
           fs.index_entry(ref(20), fs.file_name("corrupt.txt", ref(ROOT), len(CORRUPT), fs.ATTR_ARCHIVE)),
           fs.index_entry(ref(17), fs.file_name("frag.bin", ref(ROOT), 10000, fs.ATTR_ARCHIVE)),
           fs.index_entry(ref(16), fs.file_name("hello.txt", ref(ROOT), len(HELLO), fs.ATTR_ARCHIVE)),
           fs.index_entry(ref(19), fs.file_name("r\u00e9sum\u00e9.txt", ref(ROOT), len(RESUME), fs.ATTR_ARCHIVE)),
           fs.index_entry(ref(USERS), fs.file_name("Users", ref(ROOT), 0, fs.ATTR_DIR)),
           fs.index_end()])
    u_entries = b"".join([
        fs.index_entry(ref(21), fs.file_name("alice.txt", ref(USERS), len(ALICE), fs.ATTR_ARCHIVE)),
        fs.index_entry(ref(22), fs.file_name("logs", ref(USERS), 0, fs.ATTR_DIR)),
        fs.index_end()])
    l_entries = fs.index_entry(ref(23), fs.file_name("boot.log", ref(22), len(BOOTLOG), fs.ATTR_ARCHIVE)) + fs.index_end()
    recs = {
        0: fs.record(0, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.META)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("$MFT", ref(ROOT), 0, fs.META)),
            fs.attr_nres(fs.ATTR_DATA, fs.enc_runs([(MFT_CLUSTERS, MFT_LCN)]),
                         MFT_CLUSTERS * fs.CLUSTER, MFT_CLUSTERS * fs.CLUSTER, MFT_CLUSTERS * fs.CLUSTER, aid=1),
            fs.attr_res(fs.ATTR_BITMAP, bytes([0xFF, 0xFF, 0xFF]), aid=2),
        ]),
        1: fs.record(1, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.META)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("$MFTMirr", ref(ROOT), 0, fs.META)),
            fs.attr_nres(fs.ATTR_DATA, fs.enc_runs([(1, 10)]), fs.CLUSTER, fs.CLUSTER, fs.CLUSTER, aid=1),
        ]),
        2: fs.record(2, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.META)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("$LogFile", ref(ROOT), 0, fs.META)),
            fs.attr_nres(fs.ATTR_DATA, fs.enc_runs([(1, 11)]), fs.CLUSTER, fs.CLUSTER, fs.CLUSTER, aid=1),
        ]),
        3: fs.record(3, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.META)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("$Volume", ref(ROOT), 0, fs.META)),
            fs.attr_res(fs.ATTR_VOLUME_NAME, "FANTUANNTFS".encode('utf-16-le'), aid=1),
            fs.attr_res(fs.ATTR_VOLUME_INFO, struct.pack('<QBBH', 0, 3, 1, 0), aid=2),
            fs.attr_res(fs.ATTR_DATA, b"", aid=3),
        ]),
        4: fs.record(4, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.META)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("$AttrDef", ref(ROOT), 0, fs.META)),
            fs.attr_res(fs.ATTR_DATA, b"", aid=1),
        ]),
        ROOT: fs.record(ROOT, 3, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.ATTR_DIR)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name(".", ref(ROOT), 0, fs.ATTR_DIR)),
            fs.attr_res(fs.ATTR_INDEX_ROOT, fs.index_root(b"", large_child=0), name=fs.I30, aid=2),
            fs.attr_nres(fs.ATTR_INDEX_ALLOC, fs.enc_runs([(1, 13)]), fs.CLUSTER, fs.CLUSTER, fs.CLUSTER,
                         name=fs.I30, aid=3),
            fs.attr_res(fs.ATTR_BITMAP, b'\x01\0\0\0\0\0\0\0', name=fs.I30, aid=4),
        ]),
        6: fs.record(6, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.META)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("$Bitmap", ref(ROOT), 0, fs.META)),
            fs.attr_nres(fs.ATTR_DATA, fs.enc_runs([(1, 14)]), fs.CLUSTER, 256, 256, aid=1),
        ]),
        7: fs.record(7, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.META)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("$Boot", ref(ROOT), 0, fs.META)),
            fs.attr_nres(fs.ATTR_DATA, fs.enc_runs([(2, 0)]), 8192, 8192, 8192, aid=1),
        ]),
        8: fs.record(8, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.META)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("$BadClus", ref(ROOT), 0, fs.META)),
            fs.attr_res(fs.ATTR_DATA, b"", aid=1),
            fs.attr_nres(fs.ATTR_DATA, fs.enc_runs([(NCLU, None)]), NCLU * fs.CLUSTER, NCLU * fs.CLUSTER, 0,
                         name="$Bad", aid=2),
        ]),
        9: fs.record(9, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.META)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("$Secure", ref(ROOT), 0, fs.META)),
            fs.attr_res(fs.ATTR_DATA, b"", aid=1),
        ]),
        10: fs.record(10, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.META)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("$UpCase", ref(ROOT), 0x20000, fs.META)),
            fs.attr_nres(fs.ATTR_DATA, fs.enc_runs([(32, 30)]), 0x20000, 0x20000, 0x20000, aid=1),
        ]),
        11: fs.record(11, 3, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.META | fs.ATTR_DIR)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("$Extend", ref(ROOT), 0, fs.META | fs.ATTR_DIR)),
            fs.attr_res(fs.ATTR_INDEX_ROOT, fs.index_root(fs.index_end()), name=fs.I30, aid=2),
        ]),
        16: fs.record(16, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.ATTR_ARCHIVE)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("hello.txt", ref(ROOT), len(HELLO), fs.ATTR_ARCHIVE)),
            fs.attr_res(fs.ATTR_DATA, HELLO, aid=1),
        ]),
        17: fs.record(17, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.ATTR_ARCHIVE)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("frag.bin", ref(ROOT), 10000, fs.ATTR_ARCHIVE)),
            fs.attr_nres(fs.ATTR_DATA, fs.enc_runs([(1, 16), (1, 18), (1, 20)]), 3 * fs.CLUSTER, 10000, 10000, aid=1),
        ]),
        USERS: fs.record(USERS, 3, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.ATTR_DIR)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("Users", ref(ROOT), 0, fs.ATTR_DIR)),
            fs.attr_res(fs.ATTR_INDEX_ROOT, fs.index_root(u_entries), name=fs.I30, aid=2),
        ]),
        19: fs.record(19, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.ATTR_ARCHIVE)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("r\u00e9sum\u00e9.txt", ref(ROOT), len(RESUME), fs.ATTR_ARCHIVE)),
            fs.attr_res(fs.ATTR_DATA, RESUME, aid=1),
        ]),
        20: fs.record(20, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.ATTR_ARCHIVE)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("corrupt.txt", ref(ROOT), len(CORRUPT), fs.ATTR_ARCHIVE)),
            fs.attr_res(fs.ATTR_DATA, CORRUPT, aid=1),
        ]),
        21: fs.record(21, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.ATTR_ARCHIVE)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("alice.txt", ref(USERS), len(ALICE), fs.ATTR_ARCHIVE)),
            fs.attr_res(fs.ATTR_DATA, ALICE, aid=1),
        ]),
        22: fs.record(22, 3, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.ATTR_DIR)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("logs", ref(USERS), 0, fs.ATTR_DIR)),
            fs.attr_res(fs.ATTR_INDEX_ROOT, fs.index_root(l_entries), name=fs.I30, aid=2),
        ]),
        23: fs.record(23, 1, [
            fs.attr_res(fs.ATTR_STANDARD_INFO, fs.standard_info(fs.ATTR_ARCHIVE)),
            fs.attr_res(fs.ATTR_FILE_NAME, fs.file_name("boot.log", ref(22), len(BOOTLOG), fs.ATTR_ARCHIVE)),
            fs.attr_res(fs.ATTR_DATA, BOOTLOG, aid=1),
        ]),
    }
    recs[20] = bytearray(recs[20])
    struct.pack_into('<H', recs[20], 2 * fs.SECTOR - 2, 0xBEEF)  # break the fixup
    recs[20] = bytes(recs[20])

    mft = bytearray(MFT_CLUSTERS * fs.CLUSTER)
    for n, r in recs.items():
        mft[n * fs.REC:(n + 1) * fs.REC] = r
    wdata(MFT_LCN, mft)
    mirr = bytearray(fs.CLUSTER)
    for n in range(4):
        mirr[n * fs.REC:(n + 1) * fs.REC] = recs[n]
    wdata(10, mirr)
    wdata(13, fs.indx(0, root_entries))
    wdata(30, fs.upcase_table())
    frag = frag_data()
    frag += b"\0" * (3 * fs.CLUSTER - len(frag))
    wdata(16, frag[0:fs.CLUSTER])
    wdata(18, frag[fs.CLUSTER:2 * fs.CLUSTER])
    wdata(20, frag[2 * fs.CLUSTER:3 * fs.CLUSTER])
    used = [0, 1, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 16, 18, 20] + list(range(30, 62)) + [NCLU - 1]
    bm = bytearray(256)
    for c in used:
        bm[c // 8] |= 1 << (c % 8)
    wdata(14, bm)
    boot = fs.boot_sector(total, MFT_LCN, 10)
    wsect(NTFS_LBA, boot)
    wsect(NTFS_LBA + total - 1, boot)


def shell_cmd():
    """The ESP autorun transcript for the NTFS phase (12 lines max)."""
    lines = [
        b"lsmnt",
        b"lsos",
        b"ls /mnt/win0",
        b"ls /mnt/win0/Users",
        b"cat /mnt/win0/hello.txt",
        b"cat /mnt/win0/frag.bin",
        b"cat /mnt/win0/r\xc3\xa9sum\xc3\xa9.txt",
        b"cat /mnt/win0/Users/alice.txt",
        b"cat /mnt/win0/corrupt.txt",
        b"cat /mnt/win0/missing.txt",
        b"sh -c 'ls /mnt/win0/Users/logs;cat /mnt/win0/hello.txt;echo x > /mnt/win0/new.txt'",
    ]
    return b"".join(l + b"\n" for l in lines)


def main(argv):
    if "--extract" in argv:
        out = argv[argv.index("--extract") + 1]
        os.makedirs(out, exist_ok=True)
        for name, data in files().items():
            path = os.path.join(out, name)
            os.makedirs(os.path.dirname(path) or out, exist_ok=True)
            with open(path, "wb") as f:
                f.write(data)
        print("extracted %d fixture files to %s" % (len(files()), out))
        return 0
    print(__doc__.strip())
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
