#!/usr/bin/env bash
# P1 POSIX smoke: build libc-fantuan + the C hello, boot the minimal kernel
# (no tools/net) with the hello embedded, and assert the loader + fd syscall
# pipeline: SysV argv, printf to /dev/console, brk malloc, tmpfs round trip,
# pipe, clock and the exit/reap. See docs/POSIX_PLAN.md.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build

echo "[1/4] building libc-fantuan + hello (deterministic)..."
if ! ./tools/build-libc.sh --verify > build/posix-libc-build.log 2>&1; then
  echo "SMOKE-POSIX FAIL — libc build:"; tail -20 build/posix-libc-build.log; exit 1
fi
grep -q "deterministic:" build/posix-libc-build.log \
  || { echo "SMOKE-POSIX FAIL — no determinism proof"; exit 1; }

echo "[2/4] building the minimal kernel with the hello embedded..."
python3 tools/kconfig.py --profile minimal > /dev/null
if ! ./tools/build.sh > build/posix-kernel-build.log 2>&1; then
  echo "SMOKE-POSIX FAIL — kernel build:"; tail -20 build/posix-kernel-build.log; exit 1
fi

echo "[3/4] booting QEMU (bounded)..."
rm -f build/smoke-posix.log
timeout --signal=KILL 90 ./tools/run.sh > build/smoke-posix.log < /dev/null 2>&1 || true

echo "[4/4] asserting the C program's markers..."
ok=1
check() { # marker
  if grep -qa "$1" build/smoke-posix.log; then
    echo "  ok: $1"
  else
    echo "  MISSING: $1"; ok=0
  fi
}
grep -qaE "user: C hello ELF [0-9]+ bytes -> tid [0-9]+ \(libc-fantuan" build/smoke-posix.log \
  && echo "  ok: hello ELF embedded and spawned" || { echo "  MISSING: hello spawn line"; ok=0; }
check "user: hello from C (argc=1 argv0=/bin/hello)"
check "user: pid"
check "user: malloc brk (realloc=ok)"
check "user: cwd /"
check "user: tmpfs /tmp/hello.txt size="
check "read=29 match=1"
check "user: pipe write=4 read=4 data=pipe"
check "user: uptime"
check "user: exit code 0"
HELLO_TID="$(grep -aoE "user: C hello ELF [0-9]+ bytes -> tid [0-9]+" build/smoke-posix.log | grep -oE "[0-9]+$" | tail -1)"
if [ -n "$HELLO_TID" ]; then
  check "sched: reaped tid $HELLO_TID"
else
  echo "  MISSING: hello tid (spawn line)"; ok=0
fi
! grep -qa "user: .*failed" build/smoke-posix.log || { echo "  FAIL: a C hello step reported failed"; ok=0; }
! grep -qa "user fault" build/smoke-posix.log || { echo "  FAIL: user fault during the C run"; ok=0; }

if [ "$ok" = "1" ]; then
  echo "SMOKE-POSIX PASS (ELF loader + v2 syscalls + libc-fantuan, minimal profile)"
  grep -aE "user: C hello|user: hello from C|user: tmpfs|user: pipe|user: exit|reaped tid 6" build/smoke-posix.log
  exit 0
fi
echo "SMOKE-POSIX FAIL — log tail:"
tail -40 build/smoke-posix.log
exit 1
