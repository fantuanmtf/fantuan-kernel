#!/usr/bin/env bash
# Headless boot smoke test: run QEMU for a bounded time, expect the handshake.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build
rm -f build/smoke.log
timeout --signal=KILL 60 ./tools/run.sh > build/smoke.log 2>&1 || true
if grep -q "handshake ok" build/smoke.log && grep -q "beep: boot ok" build/smoke.log && grep -q "frame self-test ok" build/smoke.log && grep -q "demo tasks spawned" build/smoke.log && grep -q "task 3" build/smoke.log && grep -q "userland: hello" build/smoke.log && grep -q "userland: tid 5 exiting" build/smoke.log && grep -q "ahci: LBA0 read ok" build/smoke.log && grep -q "vfs: HELLO.TXT" build/smoke.log && grep -q "vfs: INFO.TXT => 1000" build/smoke.log && grep -q "esp: EFI/BOOT/BOOTX64.EFI" build/smoke.log && grep -q "fstab PARTUUID matches partition 1" build/smoke.log && grep -q "UUID matches grub.cfg" build/smoke.log && grep -q "nvram: BootCurrent" build/smoke.log && grep -q "BootOrder 5 entries" build/smoke.log && grep -q "repair: FIXED.TXT write+readback ok" build/smoke.log; then
  echo "SMOKE PASS"
  grep -E "handshake ok|frame allocator|frame self-test|syscall:|sched:|user: ELF|userland: hello|userland: tid 5|beep: boot ok" build/smoke.log | head -10
else
  echo "SMOKE FAIL — log tail:"
  tail -40 build/smoke.log
  exit 1
fi

# Second phase (M7.5b): a broken ESP (no EFI/BOOT/BOOTX64.EFI) must be
# repaired by copying the shim into place.
rm -f build/smoke-broken.log
timeout --signal=KILL 60 ./tools/run.sh --broken > build/smoke-broken.log 2>&1 || true
if grep -q "repair: FIXED.TXT write+readback ok" build/smoke-broken.log && grep -q "repair: copied EFI/ubuntu/shimx64.efi -> EFI/BOOT/BOOTX64.EFI" build/smoke-broken.log; then
  echo "SMOKE PASS (broken-ESP repair)"
  grep -E "repair: FIXED|fallback loader MISSING|repair: copied" build/smoke-broken.log
else
  echo "SMOKE FAIL (broken-ESP repair) — log tail:"
  tail -30 build/smoke-broken.log
  exit 1
fi
