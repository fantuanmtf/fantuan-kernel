#!/usr/bin/env bash
# RISC-V smoke: bounded QEMU virt + OpenSBI runs. Three phases:
#   A (read-only): boot the two-filesystem disk, drive the shell over the
#     serial console, assert the boot/VFS/diagnosis/userland evidence and
#     that nothing was written.
#   B (repair YES): broken-ESP disk, `grub-fix repair` + YES — the FAT
#     writes (FIXED.TXT self-test, fallback shim copy) must complete over
#     virtio-blk.
#   C (repair NO): same disk, answer NO — the gate must abort with nothing
#     written.
# Skips (clearly) when qemu-system-riscv64 is absent.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v qemu-system-riscv64 >/dev/null 2>&1; then
  echo "SMOKE SKIP (riscv64): qemu-system-riscv64 not installed"
  exit 0
fi

LOG="build/smoke-riscv.log"
rm -f "$LOG"
# Feed the shell commands through the serial console (the pipe is kept open
# so QEMU does not see EOF); each run is bounded by its timeout.
(sleep 5; printf 'diskhealth\ncat HELLO.TXT\n'; sleep 70) \
  | timeout --signal=KILL 60 ./tools/run.sh --arch riscv64 --disk --two-fs > "$LOG" 2>&1 || true

if grep -q "fantuan v0.0.1 (riscv64)" "$LOG" \
   && grep -q "boot: hartid=" "$LOG" \
   && grep -q "dtb=0x" "$LOG" \
   && grep -q "fdt: memory 0x80000000" "$LOG" \
   && grep -q "paging: Sv39 tables built" "$LOG" \
   && grep -q "high half online" "$LOG" \
   && grep -q "mm: frame self-test ok" "$LOG" \
   && grep -q "trap: ebreak handled" "$LOG" \
   && grep -q "timer: SBI timer armed" "$LOG" \
   && grep -q "tick: 10 s (SBI timer" "$LOG" \
   && grep -q "sched: 2 riscv kernel tasks spawned" "$LOG" \
   && grep -q "task 1 (tid 1): hello" "$LOG" \
   && grep -q "user: 2 riscv user tasks spawned" "$LOG" \
   && grep -q "cpu: .*model=riscv-virtio" "$LOG" \
   && grep -q "userland: hello from tid" "$LOG" \
   && grep -q "userland: tid .* exiting" "$LOG" \
   && grep -q "userland: tid .* fault test" "$LOG" \
   && grep -q "killing user task" "$LOG" \
   && grep -q "sched: reaped tid 3" "$LOG" \
   && grep -q "sched: reaped tid 4" "$LOG" \
   && grep -q "blk: virtio registered" "$LOG" \
   && grep -q "vfs: HELLO.TXT =>" "$LOG" \
   && grep -q "ext4: mounted ro at /mnt/root0" "$LOG" \
   && grep -q "probe: part 3 XFS identified" "$LOG" \
   && grep -q "vfs: ready" "$LOG" \
   && grep -q "bootrepair: v1 diagnosis" "$LOG" \
   && grep -q "bootrepair: runtime services unavailable" "$LOG" \
   && grep -q "shell: ready" "$LOG" \
   && grep -q "SMART unsupported for this transport (virtio)" "$LOG" \
   && grep -q "Hello from the fantuan-kernel VFS!" "$LOG" \
   && ! grep -q "repair: FIXED.TXT write" "$LOG"; then
  echo "SMOKE PASS (riscv64 phase A: boot, Sv39, traps, timer, userland+X, virtio-blk, VFS, shell, read-only boot)"
  grep -aE "fantuan v0.0.1 \(riscv64|mm: frame self-test|trap: |timer: |tick: |sched: |task [12] |user: |userland: |cpu: |^exc |blk: |vfs: |ext4: |probe: |bootrepair: |shell|SMART|Hello from" "$LOG" | head -40 || true
else
  echo "SMOKE FAIL (riscv64 phase A) — log tail:"
  tail -20 "$LOG"
  exit 1
fi

# Phase B: consent-gated repair writes over virtio-blk.
RLOG="build/smoke-riscv-repair.log"
rm -f "$RLOG"
(sleep 5; printf 'grub-fix repair\nYES\n'; sleep 70) \
  | timeout --signal=KILL 60 ./tools/run.sh --arch riscv64 --disk --broken > "$RLOG" 2>&1 || true

if grep -q "grub-fix: repair mode ON" "$RLOG" \
   && grep -q "repair: FIXED.TXT write+readback ok" "$RLOG" \
   && grep -q "repair: copied EFI/ubuntu/shimx64.efi -> EFI/BOOT/BOOTX64.EFI (21 bytes, verified)" "$RLOG" \
   && grep -q "repair: done" "$RLOG"; then
  echo "SMOKE PASS (riscv64 phase B: repair YES — virtio FAT writes, fallback copy)"
  grep -aE "grub-fix: repair mode ON|repair: FIXED|repair: copied|repair: done" "$RLOG" | head -6 || true
else
  echo "SMOKE FAIL (riscv64 phase B repair) — log tail:"
  tail -20 "$RLOG"
  exit 1
fi

# Phase C: the YES gate must abort with nothing written.
NLOG="build/smoke-riscv-repair-no.log"
rm -f "$NLOG"
(sleep 5; printf 'grub-fix repair\nNO\n'; sleep 70) \
  | timeout --signal=KILL 60 ./tools/run.sh --arch riscv64 --disk --broken > "$NLOG" 2>&1 || true

if grep -q "grub-fix: confirmation not YES — aborted (nothing written)" "$NLOG" \
   && ! grep -q "repair: FIXED.TXT write" "$NLOG"; then
  echo "SMOKE PASS (riscv64 phase C: repair NO — gate aborts, nothing written)"
else
  echo "SMOKE FAIL (riscv64 phase C repair gate) — log tail:"
  tail -20 "$NLOG"
  exit 1
fi
