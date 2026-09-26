#!/usr/bin/env bash
# P2 dash smoke (docs/POSIX_PLAN.md): build libc-fantuan + the vendored dash
# and the first-party /bin tools, boot the **minimal** kernel (no tools/net)
# with the dash/tools embedded, and drive a paced serial feeder through a
# non-interactive session and an interactive one on /dev/console. Asserts the
# transcripts, exit statuses, the SIGINT handler and the reaps.
#
# P3: `sh` prefers the embedded bash, so this smoke selects dash explicitly
# (the `dash` console command and /bin/dash) to keep the P2 shell covered.
#
#   ./tools/smoke-dash.sh            # bounded; exits non-zero on any miss
#   DASH_VERIFY=1 ./tools/smoke-dash.sh   # also prove the dash build is
#                                          byte-reproducible (adds ~13 s)
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build

echo "[1/5] building libc-fantuan + the C userland..."
if ! ./tools/build-libc.sh > build/smoke-dash-libc.log 2>&1; then
  echo "SMOKE-DASH FAIL — libc build:"; tail -20 build/smoke-dash-libc.log; exit 1
fi

echo "[2/5] building dash (tools/build-dash.sh)..."
if [ "${DASH_VERIFY:-0}" = "1" ]; then
  if ! DASH_VERIFY=1 ./tools/build-dash.sh > build/smoke-dash-build.log 2>&1; then
    echo "SMOKE-DASH FAIL — dash build:"; tail -20 build/smoke-dash-build.log; exit 1
  fi
  grep -q "deterministic:" build/smoke-dash-build.log \
    || { echo "SMOKE-DASH FAIL — no dash determinism proof"; exit 1; }
else
  if ! ./tools/build-dash.sh > build/smoke-dash-build.log 2>&1; then
    echo "SMOKE-DASH FAIL — dash build:"; tail -20 build/smoke-dash-build.log; exit 1
  fi
fi

echo "[3/5] building the minimal kernel with dash + /bin/ls + /bin/cat..."
python3 tools/kconfig.py --profile minimal > /dev/null
if ! ./tools/build.sh > build/smoke-dash-kernel.log 2>&1; then
  echo "SMOKE-DASH FAIL — kernel build:"; tail -20 build/smoke-dash-kernel.log; exit 1
fi

echo "[4/5] booting QEMU with the paced feeder (bounded)..."
LOG="build/smoke-dash.log"
rm -f "$LOG"
feeder=$(cat <<'PY'
import sys, time
log = sys.argv[1]

def seen(pat):
    try:
        with open(log, "rb") as fh:
            return pat in fh.read().decode("utf-8", "replace")
    except FileNotFoundError:
        return False

def wait(pat, tries=240):
    for _ in range(tries):
        if seen(pat):
            return True
        time.sleep(0.25)
    return False

def send(s, d=0.005):
    for ch in s:
        sys.stdout.write(ch)
        sys.stdout.flush()
        time.sleep(d)

def ensure(cmd, pat):
    for _ in range(6):
        send(cmd + "\n")
        if wait(pat, 40):
            return True
    return False

def run(cmd, pause=0.8):
    send(cmd + "\n")
    time.sleep(pause)

wait("shell: ready", 480)
# P4: the console lands in the login shell (bash); dash is the fallback shell
# and stays selectable from it as /bin/dash.
wait("shell: login shell: bash pid", 240)
time.sleep(1.0)

# Non-interactive: -c scripts, arithmetic/substitution, a clean and a
# non-zero exit.
ensure("dash -c 'echo noninteractive-ok'", "noninteractive-ok")
ensure("dash -c 'exit 7'; echo status=$?", "status=7")

# Interactive: prompt, line editing, builtins, fork/exec, pipes, redirects
# and scripts - dash launched from the login shell.
send("dash\n")
time.sleep(1.5)
ensure("echo interactive-ok", "interactive-ok")
ensure("echo arith=$((1+2))", "arith=3")
ensure("echo sub=$(echo sub)", "sub=sub")
ensure("echo hello-pipe | cat", "hello-pipe")
run("echo redir-ok > /tmp/f")
ensure("cat < /tmp/f", "redir-ok")
run('echo "echo from-script" > /tmp/s.sh')
ensure("dash /tmp/s.sh", "from-script")
ensure("ls /tmp", "s.sh")
ensure("false", "")
ensure("echo status=$?", "status=1")
send("echo aaaa\x7f\x7f\x7f\x7fbs-ok\n")
wait("bs-ok", 40)
send("read x\n")
time.sleep(1.0)
sys.stdout.write("\x03")
sys.stdout.flush()
time.sleep(1.0)
ensure("echo back=$?", "back=130")
send("exit\n")
time.sleep(1.5)

# P4 fallback: `exit` leaves the login shell for the built-in rescue shell,
# where the P2 console command path (and the reap status it logs) lives.
send("exit\n")
wait("shell: login shell exited", 120)
time.sleep(1.0)
ensure("dash -c 'echo dash-builtin-path'", "dash-builtin-path")
send("dash -c 'exit 7'\n")
wait("wait status=0x700", 90)
PY
)
python3 -c "$feeder" "$LOG" \
  | timeout --signal=KILL 90 ./tools/run.sh > "$LOG" 2>&1 || true

echo "[5/5] asserting the dash transcripts..."
ok=1
check() { # marker
  if grep -qa "$1" "$LOG"; then
    echo "  ok: $1"
  else
    echo "  MISSING: $1"; ok=0
  fi
}
check "p2: process layer ready"
check "shell: login shell: bash pid"
check "shell: login shell exited"
check "sh: dash pid"
check "noninteractive-ok"
check "status=7"
check "dash-builtin-path"
check "wait status=0x700"
check "interactive-ok"
check "arith=3"
check "sub=sub"
check "hello-pipe"
check "redir-ok"
check "from-script"
check "status=1"
check "bs-ok"
check "back=130"
grep -qaE "sh: dash pid [0-9]+ exited, wait status=0x0" "$LOG" \
  && echo "  ok: interactive dash exited" || { echo "  MISSING: interactive dash exit"; ok=0; }
REAPS="$(grep -acE "sched: reaped tid [0-9]+" "$LOG")"
if [ "$REAPS" -ge 4 ]; then
  echo "  ok: reaped tasks ($REAPS)"
else
  echo "  MISSING: reaps (got $REAPS)"; ok=0
fi
if grep -qa "shell: unknown command 'echo'" "$LOG"; then
  echo "  FAIL: interactive input leaked to the built-in shell"; ok=0
fi

if [ "$ok" = "1" ]; then
  echo "SMOKE-DASH PASS (dash 0.5.12: -c, interactive, pipes, redirects, scripts, SIGINT, reaps)"
  grep -aE "sh: dash pid|noninteractive-ok|arith=3|sub=sub|hello-pipe|redir-ok|from-script|status=1|back=130|SMOKE" "$LOG" | head -20
  exit 0
fi
echo "SMOKE-DASH FAIL — log tail:"
tail -40 "$LOG"
exit 1
