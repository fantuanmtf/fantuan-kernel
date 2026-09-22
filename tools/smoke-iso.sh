#!/usr/bin/env bash
# M10-6 ISO smoke: build build/fantuan.iso once, then boot the *same* hybrid
# image on both firmware paths with QEMU's CD-ROM device (-cdrom):
#   phase 1: SeaBIOS El Torito hard-disk emulation -> BIOS chain -> shell
#   phase 2: OVMF reads the platform 0xEF FAT image -> UEFI chain -> VFS/shell
# The OVMF invocation mirrors tools/run.sh; the AHCI disk is built with
# --broken so the test disk's dummy fallback loader cannot win the boot.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build

# The CD firmware-preload stage2 has a low-RAM budget: this boot-path smoke
# selects the minimal profile regardless of the caller's .config (net/TLS
# kernels belong to the UEFI/disk paths and exceed the preload window).
python3 tools/kconfig.py --profile minimal >/dev/null || exit 1

./tools/build-iso.sh || exit 1

echo "[phase 1] BIOS El Torito boot (SeaBIOS -cdrom)..."
LOG=build/smoke-iso-bios.log
rm -f "$LOG"
timeout --signal=KILL 60 qemu-system-x86_64 -machine pc -m 512M \
  -cdrom build/fantuan.iso -nographic -no-reboot \
  < /dev/null > "$LOG" 2>&1 || true
if grep -q "fantuan-bios stage2 (M10-3)" "$LOG" \
   && grep -q "handshake ok: magic=0x46544e46 version=2" "$LOG" \
   && grep -q "shell: ready" "$LOG"; then
  echo "SMOKE PASS (ISO BIOS: El Torito no-emulation preload -> x86_64 shell)"
  grep -aE "fantuan-bios stage2|handshake ok|shell: ready" "$LOG" | head -4
else
  echo "SMOKE FAIL (ISO BIOS) - log tail:"
  tail -25 "$LOG"
  exit 1
fi

echo "[phase 2] UEFI El Torito boot (OVMF -cdrom, platform id 0xEF)..."
OVMF_DIR=""
for d in /usr/share/edk2/x64 /usr/share/OVMF /usr/share/ovmf /usr/share/edk2-ovmf /usr/share/edk2/OvmfX64; do
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
cp "$OVMF_VARS" build/OVMF_VARS.iso.fd

python3 tools/mkdisk.py --broken build/test.img > /dev/null
LOG2=build/smoke-iso-uefi.log
rm -f "$LOG2"
timeout --signal=KILL 90 qemu-system-x86_64 -machine pc -m 512M -cpu max \
  -drive if=pflash,format=raw,readonly=on,file="$OVMF_CODE" \
  -drive if=pflash,format=raw,file=build/OVMF_VARS.iso.fd \
  -cdrom build/fantuan.iso \
  -device ich9-ahci,id=sata \
  -drive file=build/test.img,format=raw,if=none,id=td0 \
  -device ide-hd,drive=td0,bus=sata.0 \
  -nographic -no-reboot -no-shutdown \
  < /dev/null > "$LOG2" 2>&1 || true
if grep -q "fantuan-boot v0.0.4" "$LOG2" \
   && grep -q "kernel: loaded at 0x1000000" "$LOG2" \
   && grep -q "handshake ok: magic=0x46544e46" "$LOG2" \
   && grep -q "vfs: mounted FAT32 at /mnt/disk0" "$LOG2" \
   && grep -q "vfs: HELLO.TXT =>" "$LOG2" \
   && grep -q "shell: ready" "$LOG2"; then
  echo "SMOKE PASS (ISO UEFI: OVMF mounts the 0xEF FAT image, kernel VFS + shell)"
  grep -aE "fantuan-boot|kernel: loaded|vfs: mounted|vfs: HELLO|shell: ready" "$LOG2" | head -6
else
  echo "SMOKE FAIL (ISO UEFI) - log tail:"
  tail -30 "$LOG2"
  exit 1
fi
