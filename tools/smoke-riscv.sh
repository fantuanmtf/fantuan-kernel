#!/usr/bin/env bash
# RISC-V smoke: bounded QEMU virt + OpenSBI run; expects the S-mode banner
# and the handoff report. Skips (clearly) when qemu-system-riscv64 is absent.
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
# Feed the shell a couple of commands through the serial console (kept open
# so QEMU does not see EOF); the run is bounded by the timeout.
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
   && grep -q "Hello from the fantuan-kernel VFS!" "$LOG"; then
  echo "SMOKE PASS (riscv64: boot, Sv39, traps, timer, userland+X, virtio-blk, VFS, shell)"
  grep -aE "fantuan v0.0.1 \(riscv64|mm: frame self-test|trap: |timer: |tick: |sched: |task [12] |user: |userland: |cpu: |^exc |blk: |vfs: |ext4: |probe: |bootrepair: |shell|SMART|Hello from" "$LOG" | head -40 || true
else
  echo "SMOKE FAIL (riscv64) — log tail:"
  tail -20 "$LOG"
  exit 1
fi
