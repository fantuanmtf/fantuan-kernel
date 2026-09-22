#!/usr/bin/env python3
"""mkdisk_raw.py — raw imager fixture images for tools/mkdisk.py:
--empty / --pattern, with the optional --badclusters range sidecar. Split out
of mkdisk.py to keep every tool file inside the size rule; no host-tool
dependencies."""

import sys

SECTOR = 512


def range_spec(spec):
    """Parse `<lba>:<count>[,<lba>:<count>...]` into an (lba, count) list."""
    ranges = []
    for item in spec.split(","):
        lba, _, count = item.partition(":")
        lba, count = int(lba), int(count)
        if count <= 0 or lba < 0:
            sys.exit(f"mkdisk: bad cluster range {item} (want lba:count, count > 0)")
        ranges.append((lba, count))
    return ranges


def raw_mode(argv):
    """Handle --empty/--pattern/--badclusters; True when one was processed."""
    if "--empty" in argv:
        idx = argv.index("--empty")
        sectors = int(argv[idx + 1])
        path = argv[idx + 2] if len(argv) > idx + 2 else "build/test.img"
        with open(path, "wb") as f:
            f.truncate(sectors * SECTOR)
        print(f"{path}: {sectors * SECTOR} bytes, empty (zero-filled), {sectors} sectors")
        return True
    if "--pattern" not in argv:
        return False
    idx = argv.index("--pattern")
    sectors = int(argv[idx + 1])
    path = argv[idx + 2] if len(argv) > idx + 2 else "build/test.img"
    with open(path, "wb") as f:
        for s in range(sectors):
            f.write(bytes(((s * 131 + j * 7) & 0xFF) for j in range(SECTOR)))
    bad = ""
    if "--badclusters" in argv:
        ranges = range_spec(argv[argv.index("--badclusters") + 1])
        for lba, count in ranges:
            if lba + count > sectors:
                sys.exit(f"mkdisk: bad cluster range {lba}:{count} outside 0..{sectors}")
        with open(path + ".bad", "w", encoding="ascii") as bf:
            for lba, count in ranges:
                bf.write(f"{lba} {count}\n")
        bad = "; bad clusters " + ",".join(f"{l}:{c}" for l, c in ranges)
        bad += f"; ranges {path}.bad"
    print(f"{path}: {sectors * SECTOR} bytes, deterministic pattern{bad}, {sectors} sectors")
    return True
