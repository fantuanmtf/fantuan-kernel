#!/usr/bin/env bash
# aarch64 smoke (M11 R9a): direct FDT boot on QEMU `virt`. One bounded
# phase: boot the raw kernel image (QEMU's Linux-compatible protocol passes
# the DTB in x0), assert the FDT/MMU/GIC/timer/task markers, the deliberate
# BRK resume, then drive `help`/`bootinfo` over the serial shell. Negative
# checks: no unexpected traps, no panic, no heartbeat after the shell.
# Skips (clearly) when qemu-system-aarch64 is absent.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v qemu-system-aarch64 >/dev/null 2>&1; then
  echo "SMOKE SKIP (aarch64): qemu-system-aarch64 not installed"
  exit 0
fi

# R9a is a minimal-profile bring-up: the shell table is help/bootinfo only.
python3 tools/kconfig.py --profile minimal >/dev/null || exit 1

LOG="build/smoke-aarch64.log"
rm -f "$LOG"
# Feed the shell through the serial console (the pipe stays open so QEMU
# does not see EOF); the run is bounded by the timeout.
(sleep 4; printf 'help\nbootinfo\n'; sleep 8) \
  | timeout --signal=KILL 30 ./tools/run.sh --arch aarch64 > "$LOG" 2>&1 || true

# C4: the heartbeat stops once the shell owns the console.
TICKS_AFTER_SHELL=$(awk '/shell: ready/{seen=1} seen && /^tick: /{n++} END{print n+0}' "$LOG")

if grep -q "fantuan v0.0.2 (aarch64) - QEMU virt" "$LOG" \
   && grep -q "boot: EL1, dtb=0x" "$LOG" \
   && grep -q "uart: pl011 up" "$LOG" \
   && grep -q "fdt: memory 0x40000000" "$LOG" \
   && grep -q "fdt: model=linux,dummy-virt" "$LOG" \
   && grep -q "fdt: uart=0x9000000 (pl011)" "$LOG" \
   && grep -qE "mm: frame allocator ready: [0-9]+ MiB usable" "$LOG" \
   && grep -q "mm: frame self-test ok (via direct map)" "$LOG" \
   && grep -q "mmu: 4K granule, direct map at 0xffff000000000000" "$LOG" \
   && grep -q "intc: GICv2 up" "$LOG" \
   && grep -q "timer: 100 Hz" "$LOG" \
   && grep -q "trap: brk handled" "$LOG" \
   && grep -q "exc: resumed after brk" "$LOG" \
   && grep -q "sched: 2 aarch64 kernel tasks spawned" "$LOG" \
   && grep -q "task 1 (tid 1): hello 0" "$LOG" \
   && grep -q "task 2 (tid 2): hello 0" "$LOG" \
   && grep -q "shell: ready (root@Fantuan-MTF" "$LOG" \
   && grep -q "root@Fantuan-MTF> " "$LOG" \
   && grep -q "shell commands (root@Fantuan-MTF" "$LOG" \
   && grep -q "this table" "$LOG" \
   && grep -q "boot handover details" "$LOG" \
   && grep -q "kernel_base 0x40080000" "$LOG" \
   && [ "$TICKS_AFTER_SHELL" -eq 0 ] \
   && ! grep -q "trap: unexpected" "$LOG" \
   && ! grep -q "fdt: parse failed" "$LOG" \
   && ! grep -q "PANIC" "$LOG"; then
  echo "SMOKE PASS (aarch64 R9a: direct FDT boot, PL011, 4K MMU + direct map, GICv2, 100 Hz, BRK resume, tasks, shell)"
  grep -aE "fantuan v0.0.2 \(aarch64|boot: |uart: |fdt: |cpu: |mm: |mmu: |intc: |timer: |trap: |exc: |sched: |task [12] |shell|help|bootinfo|kernel_base" "$LOG" | head -40 || true
else
  echo "SMOKE FAIL (aarch64) — log tail:"
  tail -30 "$LOG"
  exit 1
fi
