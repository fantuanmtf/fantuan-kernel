#!/usr/bin/env bash
# P3 bash smoke (docs/POSIX_PLAN.md): build libc-fantuan + bash 5.3 + dash and
# the first-party /bin tools, boot the **minimal** kernel with them embedded,
# and drive a paced serial feeder through bash sessions: `bash -c` scripts,
# arithmetic/variables, a pipeline, a redirection, a function, command
# substitution, an interactive prompt with ^C, the reaps, and the default `sh`
# being bash. BASH_SMOKE_SKIP_BUILD=1 reuses the existing artifacts.
#
#   ./tools/smoke-bash.sh            # bounded; exits non-zero on any miss
#   BASH_VERIFY=1 ./tools/smoke-bash.sh   # also prove the bash build is
#                                          byte-reproducible
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build

if [ "${BASH_SMOKE_SKIP_BUILD:-0}" != "1" ]; then
  echo "[1/5] building libc-fantuan + the C userland..."
  if ! ./tools/build-libc.sh > build/smoke-bash-libc.log 2>&1; then
    echo "SMOKE-BASH FAIL — libc build:"; tail -20 build/smoke-bash-libc.log; exit 1
  fi

  echo "[2/5] building dash (fallback) and bash (tools/build-{dash,bash}.sh)..."
  if ! ./tools/build-dash.sh > build/smoke-bash-dash.log 2>&1; then
    echo "SMOKE-BASH FAIL — dash build:"; tail -20 build/smoke-bash-dash.log; exit 1
  fi
  if [ "${BASH_VERIFY:-0}" = "1" ]; then
    if ! BASH_VERIFY=1 ./tools/build-bash.sh > build/smoke-bash-build.log 2>&1; then
      echo "SMOKE-BASH FAIL — bash build:"; tail -20 build/smoke-bash-build.log; exit 1
    fi
    grep -q "deterministic:" build/smoke-bash-build.log \
      || { echo "SMOKE-BASH FAIL — no bash determinism proof"; exit 1; }
  else
    if ! ./tools/build-bash.sh > build/smoke-bash-build.log 2>&1; then
      echo "SMOKE-BASH FAIL — bash build:"; tail -20 build/smoke-bash-build.log; exit 1
    fi
  fi

  echo "[3/5] building the minimal kernel with bash + dash + /bin/ls + /bin/cat..."
  python3 tools/kconfig.py --profile minimal > /dev/null
  if ! ./tools/build.sh > build/smoke-bash-kernel.log 2>&1; then
    echo "SMOKE-BASH FAIL — kernel build:"; tail -20 build/smoke-bash-kernel.log; exit 1
  fi
else
  echo "[1-3/5] reusing the existing libc/bash/dash/kernel artifacts..."
fi

echo "[4/5] booting QEMU with the paced feeder (bounded)..."
LOG="build/smoke-bash.log"
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

def ensure(cmd, pat, tries=6):
    for _ in range(tries):
        send(cmd + "\n")
        if wait(pat, 40):
            return True
    return False

def run(cmd, pause=0.8):
    send(cmd + "\n")
    time.sleep(pause)

wait("shell: ready", 480)
time.sleep(0.5)

# The default sh must be bash when embedded (dash stays selectable).
ensure("sh -c 'echo shver=${BASH_VERSION:+bash-default}'", "bash-default")

# Non-interactive: -c scripts, a clean and a non-zero exit.
ensure("bash -c 'echo noninteractive-ok'", "noninteractive-ok")
ensure("bash -c 'exit 7'", "wait status=0x700")

# Interactive bash: prompt, builtins, arithmetic, variables, fork/exec,
# pipelines, redirects, functions and command substitution.
send("bash\n")
wait("sh: bash pid", 120)
time.sleep(1.5)
ensure("echo interactive-bash-ok", "interactive-bash-ok")
ensure("echo arith=$((2+3))", "arith=5")
ensure("x=41; echo x=$((x+1))", "x=42")
ensure("f() { echo func-ok; }; f", "func-ok")
ensure("echo pipe-bash | cat", "pipe-bash")
run("echo redir-bash > /tmp/b")
ensure("cat < /tmp/b", "redir-bash")
ensure("echo sub=$(echo sub-bash)", "sub=sub-bash")
ensure("echo glob-bash-*", "glob-bash-*")
send("read x\n")
time.sleep(1.0)
sys.stdout.write("\x03")
sys.stdout.flush()
time.sleep(1.0)
ensure("echo back=$?", "back=130")
send("exit\n")
wait("exited, wait status=0x0", 60)
time.sleep(1.0)

# dash remains selectable with the same console command.
ensure("dash -c 'echo dash-still-here'", "dash-still-here")
PY
)
python3 -c "$feeder" "$LOG" \
  | timeout --signal=KILL 120 ./tools/run.sh > "$LOG" 2>&1 || true

echo "[5/5] asserting the bash transcripts..."
ok=1
check() { # marker
  if grep -qa "$1" "$LOG"; then
    echo "  ok: $1"
  else
    echo "  MISSING: $1"; ok=0
  fi
}
check "bash embedded=yes"
check "bash-default"
check "noninteractive-ok"
check "wait status=0x700"
check "sh: bash pid"
check "interactive-bash-ok"
check "arith=5"
check "x=42"
check "func-ok"
check "pipe-bash"
check "redir-bash"
check "sub=sub-bash"
check "back=130"
check "dash-still-here"
grep -qaE "sh: bash pid [0-9]+ exited, wait status=0x0" "$LOG" \
  && echo "  ok: interactive bash exited" || { echo "  MISSING: interactive bash exit"; ok=0; }
REAPS="$(grep -acE "sched: reaped tid [0-9]+" "$LOG")"
if [ "$REAPS" -ge 6 ]; then
  echo "  ok: reaped tasks ($REAPS)"
else
  echo "  MISSING: reaps (got $REAPS)"; ok=0
fi
if grep -qa "shell: unknown command 'echo'" "$LOG"; then
  echo "  FAIL: interactive input leaked to the built-in shell"; ok=0
fi

if [ "$ok" = "1" ]; then
  echo "SMOKE-BASH PASS (bash 5.3: -c, interactive, arithmetic, variables, functions, pipelines, redirects, \$(...), SIGINT, reaps; sh=bash, dash selectable)"
  grep -aE "sh: bash pid|bash-default|noninteractive-ok|arith=5|func-ok|pipe-bash|redir-bash|sub=sub-bash|back=130|dash-still-here|SMOKE" "$LOG" | head -20
  exit 0
fi
echo "SMOKE-BASH FAIL — log tail:"
tail -40 "$LOG"
exit 1
