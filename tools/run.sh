#!/usr/bin/env bash
# fantuan-kernel — one-shot build & run in QEMU/OVMF (M0).
# Usage: tools/run.sh [--graphics]   (default: headless serial console)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

GRAPHICS=0
if [ "${1:-}" = "--graphics" ]; then GRAPHICS=1; fi

echo "[1/4] building user program, kernel, bootloader..."
./tools/build.sh

mkdir -p build/esp/EFI/BOOT build/esp/fantuan
objcopy -O binary   target/x86_64-unknown-none/release/fantuan-kernel   build/esp/fantuan/kernel.bin
cp target/x86_64-unknown-uefi/release/fantuan-boot.efi   build/esp/EFI/BOOT/BOOTX64.EFI

echo "[3/4] preparing OVMF firmware..."
OVMF_DIR=""
for d in /usr/share/edk2/x64 /usr/share/OVMF /usr/share/ovmf; do
  if [ -f "$d/OVMF_CODE.4m.fd" ] || [ -f "$d/OVMF_CODE.fd" ]; then
    OVMF_DIR="$d"
    break
  fi
done
if [ -z "$OVMF_DIR" ]; then
  echo "error: OVMF firmware not found (install ovmf/edk2-ovmf)" >&2
  exit 1
fi
OVMF_CODE="$OVMF_DIR/OVMF_CODE.4m.fd"
OVMF_VARS="$OVMF_DIR/OVMF_VARS.4m.fd"
[ -f "$OVMF_CODE" ] || OVMF_CODE="$OVMF_DIR/OVMF_CODE.fd"
[ -f "$OVMF_VARS" ] || OVMF_VARS="$OVMF_DIR/OVMF_VARS.fd"
[ -f build/OVMF_VARS.fd ] || cp "$OVMF_VARS" build/OVMF_VARS.fd

echo "[4/4] preparing AHCI test disk + starting QEMU..."
# GPT + FAT32 test disk for the C AHCI driver and the VFS (tools/mkdisk.py).
python3 tools/mkdisk.py build/test.img
AHCI_DEV="-device ich9-ahci,id=sata -drive file=build/test.img,format=raw,if=none,id=td0 -device ide-hd,drive=td0,bus=sata.0"

if [ "$GRAPHICS" = "1" ]; then
  exec qemu-system-x86_64 -m 512M \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
    -drive if=pflash,format=raw,file=build/OVMF_VARS.fd \
    -drive format=raw,file=fat:rw:build/esp \
    $AHCI_DEV \
    -serial stdio -no-reboot -no-shutdown
else
  exec qemu-system-x86_64 -m 512M \
    -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
    -drive if=pflash,format=raw,file=build/OVMF_VARS.fd \
    -drive format=raw,file=fat:rw:build/esp \
    $AHCI_DEV \
    -nographic -no-reboot -no-shutdown
fi
