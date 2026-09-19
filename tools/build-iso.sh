#!/usr/bin/env bash
# M10-6 hybrid BIOS+UEFI ISO builder: the x86_64 kernel + UEFI loader from
# tools/build.sh, the CD-boot BIOS image from tools/build-bios.sh --iso, the
# FAT16 ESP from tools/mkesp.py, then build/fantuan.iso from tools/mkiso.py.
# No xorriso/mkisofs/genisoimage (GPL build tools stay out of the project).
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build

echo "[1/4] building the x86_64 kernel and UEFI loader..."
./tools/build.sh >/dev/null

echo "[2/4] building the El Torito BIOS image (firmware-preload stage2)..."
./tools/build-bios.sh --iso >/dev/null

echo "[3/4] packing the FAT16 ESP image..."
python3 tools/mkesp.py --size 4M build/esp.img \
  EFI/BOOT/BOOTX64.EFI=target/x86_64-unknown-uefi/release/fantuan-boot.efi \
  FANTUAN/KERNEL.BIN=build/kernel-bios.bin

echo "[4/4] writing build/fantuan.iso..."
python3 tools/mkiso.py --output build/fantuan.iso \
  --bios build/bios-iso.img \
  --kernel build/kernel-bios.bin \
  --bootx64 target/x86_64-unknown-uefi/release/fantuan-boot.efi \
  --esp build/esp.img

python3 tools/mkiso.py --check build/fantuan.iso
SIZE=$(stat -c %s build/fantuan.iso)
if [ "$SIZE" -gt $((1024 * 1024 * 1024)) ]; then
  echo "ERROR: build/fantuan.iso is $SIZE bytes, over the 1 GiB budget" >&2
  exit 1
fi
echo "build/fantuan.iso: $SIZE bytes - budget check OK (<= 1 GiB)"
