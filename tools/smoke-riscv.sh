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
timeout --signal=KILL 45 ./tools/run.sh --arch riscv64 < /dev/null > "$LOG" 2>&1 || true

if grep -q "fantuan (riscv64) M9" "$LOG" \
   && grep -q "boot: hartid=" "$LOG" \
   && grep -q "dtb=0x" "$LOG" \
   && grep -q "fdt: memory 0x80000000" "$LOG" \
   && grep -q "paging: Sv39 tables built" "$LOG" \
   && grep -q "high half online" "$LOG" \
   && grep -q "mm: frame self-test ok" "$LOG" \
   && grep -q "trap: ebreak handled" "$LOG" \
   && grep -q "timer: SBI timer armed" "$LOG" \
   && grep -q "tick: 10 s (SBI timer" "$LOG"; then
  echo "SMOKE PASS (riscv64: boot, Sv39, traps, SBI timer)"
  grep -aE "fantuan \(riscv64|mm: usable|mm: frame self-test|trap: |timer: |tick: " "$LOG" | head -10
else
  echo "SMOKE FAIL (riscv64) — log tail:"
  tail -20 "$LOG"
  exit 1
fi
