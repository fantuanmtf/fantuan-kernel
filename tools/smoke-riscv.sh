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

if grep -q "fantuan (riscv64) M9.1" "$LOG" \
   && grep -q "boot: hartid=" "$LOG" \
   && grep -q "dtb=0x" "$LOG"; then
  echo "SMOKE PASS (riscv64: OpenSBI S-mode handoff + UART banner)"
  grep -aE "fantuan \(riscv64\)|boot: hartid" "$LOG" | head -3
else
  echo "SMOKE FAIL (riscv64) — log tail:"
  tail -20 "$LOG"
  exit 1
fi
