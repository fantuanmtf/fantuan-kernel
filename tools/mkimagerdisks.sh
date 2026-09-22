#!/usr/bin/env bash
# M12 imager fixture disks for tools/run.sh: prepares the images and prints
# the four-disk AHCI argument string on stdout.
#
# default (smoke-imager.sh): blk0 delivered test disk (ESP clone autorun),
#   blk1 1 MiB pattern source, blk2 2 MiB empty destination, blk3 512 KiB
#   empty destination (the size gate).
# --bad (smoke-imager-bad.sh): blk0 delivered test disk (bad-cluster autorun),
#   blk1 1 MiB pattern source with injected read errors, blk2 6 MiB empty
#   destination, blk3 4 MiB clean pattern source (the --quick path).
#
# Fault injection: the mkdisk --badclusters sidecar lists the bad ranges; each
# sector becomes one QEMU blkdebug `inject-error` rule (read_aio, errno EIO).
# blkdebug is a block-layer filter, so the injected error is a real AHCI read
# failure the guest driver observes; the guest never sees the host config.
# BADCLUSTERS overrides the default ranges (lba:count,...).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build

if [ "${1:-}" = "--bad" ]; then
  # Distinct names from the M12-2 fixtures so the two smokes never share a
  # destination file (the default mode leaves existing images alone).
  BAD="${BADCLUSTERS:-100:4,700:2}"
  python3 tools/mkdisk.py --imager-bad build/imager-badsrc.img >/dev/null
  python3 tools/mkdisk.py --pattern 2048 build/imager-bad.img --badclusters "$BAD" >/dev/null
  python3 tools/mkdisk.py --pattern 8192 build/imager-badclean.img >/dev/null
  python3 tools/mkdisk.py --empty 12288 build/imager-baddst.img >/dev/null
  {
    while read -r lba count; do
      for ((i = 0; i < count; i++)); do
        printf '[inject-error]\nevent = "read_aio"\nsector = "%d"\nerrno = "5"\n' "$((lba + i))"
      done
    done < build/imager-bad.img.bad
  } > build/imager-bad.cfg
  AHCI_DEV="-device ich9-ahci,id=sata"
  AHCI_DEV="$AHCI_DEV -drive file=build/imager-badsrc.img,format=raw,if=none,id=im0 -device ide-hd,drive=im0,bus=sata.0"
  AHCI_DEV="$AHCI_DEV -drive file=blkdebug:build/imager-bad.cfg:build/imager-bad.img,format=raw,if=none,id=im1 -device ide-hd,drive=im1,bus=sata.1"
  AHCI_DEV="$AHCI_DEV -drive file=build/imager-baddst.img,format=raw,if=none,id=im2 -device ide-hd,drive=im2,bus=sata.2"
  AHCI_DEV="$AHCI_DEV -drive file=build/imager-badclean.img,format=raw,if=none,id=im3 -device ide-hd,drive=im3,bus=sata.3"
  echo "$AHCI_DEV"
  exit 0
fi

# The fixture images are left alone when present so a smoke can pre-hash
# them; a manual run creates them once.
python3 tools/mkdisk.py --imager build/imager-src.img >/dev/null
python3 tools/mkdisk.py --pattern 2048 build/imager-pattern.img >/dev/null
[ -f build/imager-dst.img ] || python3 tools/mkdisk.py --empty 4096 build/imager-dst.img >/dev/null
[ -f build/imager-small.img ] || python3 tools/mkdisk.py --empty 1024 build/imager-small.img >/dev/null
AHCI_DEV="-device ich9-ahci,id=sata"
AHCI_DEV="$AHCI_DEV -drive file=build/imager-src.img,format=raw,if=none,id=im0 -device ide-hd,drive=im0,bus=sata.0"
AHCI_DEV="$AHCI_DEV -drive file=build/imager-pattern.img,format=raw,if=none,id=im1 -device ide-hd,drive=im1,bus=sata.1"
AHCI_DEV="$AHCI_DEV -drive file=build/imager-dst.img,format=raw,if=none,id=im2 -device ide-hd,drive=im2,bus=sata.2"
AHCI_DEV="$AHCI_DEV -drive file=build/imager-small.img,format=raw,if=none,id=im3 -device ide-hd,drive=im3,bus=sata.3"
echo "$AHCI_DEV"
