#!/usr/bin/env bash
# M10 BIOS smoke: build the self-written boot image and boot the x86_64
# kernel under SeaBIOS with the *default* QEMU CPU (no SMEP/SMAP) so the
# old-machine degradation paths are exercised, not hidden by -cpu max.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"

LOG="build/smoke-bios.log"
rm -f "$LOG"
./tools/build-bios.sh > /dev/null
timeout --signal=KILL 60 qemu-system-x86_64 -machine pc -m 512M \
  -drive format=raw,file=build/bios.img -nographic -no-reboot \
  < /dev/null > "$LOG" 2>&1 || true

if grep -q "fantuan-bios stage2 (M10-3)" "$LOG" \
   && grep -q "memmap: descriptors=" "$LOG" \
   && grep -q "handshake ok: magic=0x46544e46 version=2" "$LOG" \
   && grep -q "console: none (serial-only; GOP unavailable)" "$LOG" \
   && grep -q "mm: frame allocator ready" "$LOG" \
   && grep -q "mm: reclaimed 13 bootloader table pages" "$LOG" \
   && grep -q "mmu: nx true smep false smap false" "$LOG" \
   && grep -q "userland: hello from tid" "$LOG" \
   && grep -q "shell: ready" "$LOG"; then
  echo "SMOKE PASS (bios: MBR -> LBA -> E820 -> BootInfo -> ATA kernel load -> long mode -> kernel + userland + shell)"
  grep -aE "stage2|memmap: descriptors|handshake ok|console:|reclaimed|mmu:|userland: hello|shell: ready" "$LOG" | head -12 || true
else
  echo "SMOKE FAIL (bios) — log tail:"
  tail -25 "$LOG"
  exit 1
fi
