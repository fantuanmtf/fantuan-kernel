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
timeout --signal=KILL 30 ./tools/run.sh --arch riscv64 < /dev/null > "$LOG" 2>&1 || true

if grep -q "fantuan (riscv64) M9" "$LOG" \
   && grep -q "boot: hartid=" "$LOG" \
   && grep -q "dtb=0x" "$LOG" \
   && grep -q "fdt: memory 0x80000000" "$LOG" \
   && grep -q "paging: Sv39 tables built" "$LOG" \
   && grep -q "high half online" "$LOG" \
   && grep -q "mm: frame self-test ok" "$LOG"; then
  echo "SMOKE PASS (riscv64: OpenSBI handoff, FDT memory, Sv39 high half, frame self-test)"
  grep -aE "fantuan \(riscv64|fdt: memory|mm: usable|mm: frame self-test|paging: Sv39" "$LOG" | head -8
else
  echo "SMOKE FAIL (riscv64) — log tail:"
  tail -20 "$LOG"
  exit 1
fi
