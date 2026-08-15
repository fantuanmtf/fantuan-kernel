#!/usr/bin/env bash
# Headless boot smoke test: run QEMU for a bounded time, expect the handshake.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build
rm -f build/smoke.log
timeout --signal=KILL 60 ./tools/run.sh > build/smoke.log 2>&1 || true
if grep -q "handshake ok" build/smoke.log && grep -q "beep: boot ok" build/smoke.log && grep -q "frame self-test ok" build/smoke.log; then
  echo "SMOKE PASS"
  grep -E "handshake ok|memory map|frame allocator|frame self-test|beep: boot ok" build/smoke.log
else
  echo "SMOKE FAIL — log tail:"
  tail -40 build/smoke.log
  exit 1
fi
