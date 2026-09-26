#!/usr/bin/env bash
# kbd_test.sh — PS/2 keyboard + console-mirror phase (M8.5a/b).
#
# Boots the --kbd-test fixture with a QEMU monitor socket, waits for the
# shell, takes a framebuffer screendump, injects "zz" + Enter via `sendkey`,
# takes a second screendump and checks:
#   - the shell answers the unknown command (keyboard -> IRQ1 -> input), and
#   - the two screendumps differ (serial output is mirrored to the GOP).
# Prints the matching lines and exits non-zero on failure.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
# rustup-managed toolchain (system cargo lacks the no_std targets).
export PATH="$HOME/.cargo/bin:$PATH"

LOG="build/smoke-kbd.log"
BEFORE="build/screen-before.ppm"
AFTER="build/screen-after.ppm"
rm -f build/mon.sock "$LOG" "$BEFORE" "$AFTER"

mon() {
  printf '%s\n' "$1" | timeout 10 socat - UNIX-CONNECT:build/mon.sock >/dev/null 2>&1
}

# Background sender: wait until the socket exists and the shell is ready,
# then capture, inject two 'z' presses and Enter, and capture again.
(
  for _ in $(seq 1 120); do
    if [ -S build/mon.sock ] && grep -q "shell: ready" "$LOG" 2>/dev/null; then
      break
    fi
    sleep 1
  done
  sleep 3
  mon "screendump $BEFORE"
  mon "sendkey z"
  sleep 1
  mon "sendkey z"
  sleep 1
  mon "sendkey ret"
  sleep 3
  mon "screendump $AFTER"
) &
SENDER=$!

timeout --signal=KILL 150 ./tools/run.sh --kbd-test --monitor < /dev/null > "$LOG" 2>&1 || true
wait "$SENDER" 2>/dev/null || true

OK=1
grep -q "kbd: i8042 ready" "$LOG" || OK=0
# P4: the console lands in the login shell (bash), so the injected line is
# answered by bash rather than by the built-in shell's dispatcher.
grep -qaE "zz: command not found" "$LOG" || OK=0
[ -s "$BEFORE" ] && [ -s "$AFTER" ] || OK=0
cmp -s "$BEFORE" "$AFTER" && OK=0  # the mirror must have added shell output

if [ "$OK" = "1" ]; then
  echo "SMOKE PASS (PS/2 keyboard: IRQ1 scancodes reach the login shell; serial mirrored to GOP)"
  grep -aE "kbd: |console: serial output mirrored|command not found" "$LOG" | head -4
  exit 0
fi
echo "SMOKE FAIL (keyboard/console) — log tail:"
tail -20 "$LOG"
ls -la "$BEFORE" "$AFTER" 2>/dev/null || true
exit 1
