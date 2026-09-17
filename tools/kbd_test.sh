#!/usr/bin/env bash
# kbd_test.sh — PS/2 keyboard injection phase (M8.5a).
#
# Boots the --kbd-test fixture with a QEMU monitor socket, waits for the
# shell to become ready, injects "zz" + Enter via `sendkey`, and checks that
# the shell answers with an unknown-command line. Prints the matching lines
# and exits non-zero on failure. Gated by socat (available on the test host).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
# rustup-managed toolchain (system cargo lacks the no_std targets).
export PATH="$HOME/.cargo/bin:$PATH"

LOG="build/smoke-kbd.log"
rm -f build/mon.sock "$LOG"

# Background sender: wait until the socket exists and the shell is ready,
# then inject two 'z' presses and Enter. The short timeout keeps socat from
# holding the monitor connection open if the guest is already gone.
(
  for _ in $(seq 1 120); do
    if [ -S build/mon.sock ] && grep -q "shell: ready" "$LOG" 2>/dev/null; then
      break
    fi
    sleep 1
  done
  sleep 3
  { echo "sendkey z"; sleep 1; echo "sendkey z"; sleep 1; echo "sendkey ret"; } \
    | timeout 10 socat - UNIX-CONNECT:build/mon.sock >/dev/null 2>&1
) &
SENDER=$!

timeout --signal=KILL 150 ./tools/run.sh --kbd-test --monitor > "$LOG" 2>&1 || true
wait "$SENDER" 2>/dev/null || true

if grep -q "kbd: i8042 ready" "$LOG" && grep -q "shell: unknown command 'zz'" "$LOG"; then
  echo "SMOKE PASS (PS/2 keyboard: IRQ1 scancodes reach the shell)"
  grep -aE "kbd: |unknown command 'zz'" "$LOG" | head -4
  exit 0
fi
echo "SMOKE FAIL (keyboard) — log tail:"
tail -20 "$LOG"
exit 1
