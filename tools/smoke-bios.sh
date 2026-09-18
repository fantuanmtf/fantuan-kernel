#!/usr/bin/env bash
# M10 BIOS boot spike smoke: build the stage1+stage2 image and boot it under
# SeaBIOS, asserting the real-mode chain (COM1, int 0x13 LBA, E820 dump).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

LOG="build/smoke-bios.log"
rm -f "$LOG"
./tools/build-bios.sh > /dev/null
timeout --signal=KILL 15 qemu-system-x86_64 -machine pc \
  -drive format=raw,file=build/bios.img -nographic -no-reboot \
  < /dev/null > "$LOG" 2>&1 || true

if grep -q "12fantuan-bios stage2 (M10 spike)" "$LOG" \
   && grep -q "e820: entries=7" "$LOG" \
   && grep -q "e820\[0\]: base=00000000 len=0009fc00 type=00000001" "$LOG" \
   && grep -q "e820\[3\]: base=00100000 len=07ee0000 type=00000001" "$LOG" \
   && grep -q "spike: stage1+stage2+LBA read+E820 ok" "$LOG" \
   && grep -q "entering long mode..." "$LOG" \
   && grep -q "long mode ok (M10-2): 64-bit stub running" "$LOG"; then
  echo "SMOKE PASS (bios: MBR -> LBA -> stage2 -> E820 -> long mode -> 64-bit stub)"
  grep -aE "^e820|stage2|spike|entering long|long mode ok" "$LOG" | head -14 || true
else
  echo "SMOKE FAIL (bios) — log tail:"
  tail -20 "$LOG"
  exit 1
fi
