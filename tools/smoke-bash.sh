#!/usr/bin/env bash
# P3 bash smoke (docs/POSIX_PLAN.md): build libc-fantuan + bash 5.3 + dash and
# the first-party /bin tools, boot the **minimal** kernel with them embedded,
# and drive a paced serial feeder through bash sessions: `bash -c` scripts,
# arithmetic/variables, a pipeline, a redirection, a function, command
# substitution, an interactive prompt with ^C, the reaps, and the default `sh`
# being bash. Phase [0/5] proves the *default* build path (`tools/build.sh`
# alone, artifacts deleted) builds and embeds bash and bakes in its hash
# marker; BASH_SMOKE_SKIP_BUILD=1 reuses the existing artifacts.
#
#   ./tools/smoke-bash.sh            # bounded; exits non-zero on any miss
#   BASH_VERIFY=1 ./tools/smoke-bash.sh   # also prove the bash build is
#                                          byte-reproducible
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build

if [ "${BASH_SMOKE_SKIP_BUILD:-0}" != "1" ]; then
  # The point of this phase: `tools/build.sh` alone - with no helper script
  # invoked by hand and with the artifacts deleted - must fetch the sources,
  # build libc + bash + dash, embed the shell and bake in its hash marker.
  echo "[0/5] the default build path must produce and embed bash on its own..."
  rm -f kernel/bash_program.bin kernel/bash_program.sha256 build/bash/bash.elf
  python3 tools/kconfig.py --profile minimal > /dev/null
  if ! ./tools/build.sh > build/smoke-bash-default.log 2>&1; then
    echo "SMOKE-BASH FAIL — tools/build.sh:"; tail -20 build/smoke-bash-default.log; exit 1
  fi
  [ -f kernel/bash_program.bin ] \
    || { echo "SMOKE-BASH FAIL — tools/build.sh did not build bash"; exit 1; }
  [ -f kernel/bash_program.sha256 ] \
    || { echo "SMOKE-BASH FAIL — no kernel/bash_program.sha256 marker"; exit 1; }
  MARKER="$(cat kernel/bash_program.sha256)"
  [ "$(sha256sum kernel/bash_program.bin | cut -d' ' -f1)" = "$MARKER" ] \
    || { echo "SMOKE-BASH FAIL — the marker does not match the built artifact"; exit 1; }
  # No `grep -q` in the pipe: under `set -o pipefail` an early exit SIGPIPEs
  # `strings` and the successful case would read as a failure.
  strings -a target/x86_64-unknown-none/release/fantuan-kernel > build/smoke-bash-elf-strings.txt
  grep -qF "$MARKER" build/smoke-bash-elf-strings.txt \
    || { echo "SMOKE-BASH FAIL — the kernel ELF does not carry the shell marker"; exit 1; }
  grep -q "shell: image sha256 $MARKER" build/smoke-bash-default.log \
    || echo "  note: the boot-time shell marker line was not in the build log"
  echo "  default build embedded bash ($MARKER)"

  if [ "${BASH_VERIFY:-0}" = "1" ]; then
    if ! BASH_VERIFY=1 ./tools/build-bash.sh > build/smoke-bash-build.log 2>&1; then
      echo "SMOKE-BASH FAIL — bash build:"; tail -20 build/smoke-bash-build.log; exit 1
    fi
    grep -q "deterministic:" build/smoke-bash-build.log \
      || { echo "SMOKE-BASH FAIL — no bash determinism proof"; exit 1; }
  fi

  echo "[1-3/5] libc, bash, dash and the kernel came from that one build; reusing..."
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
# P4: the console lands in the login shell, and that shell must be bash.
wait("shell: login shell: bash pid", 240)
time.sleep(1.0)

# /bin/sh and /bin/bash both resolve to the embedded bash.
ensure("sh -c 'echo shver=${BASH_VERSION:+bash-default}'", "bash-default")
ensure("bash -c 'echo noninteractive-ok'", "noninteractive-ok")
ensure("bash -c 'exit 7'; echo status=$?", "status=7")

# Interactive bash: this *is* the login shell, so the tests run straight in it -
# builtins, arithmetic, variables, fork/exec, pipelines, redirects, functions
# and command substitution.
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

# P4 fallback: `exit` leaves the login shell for the built-in rescue shell,
# where the P2/P3 console commands live - and both shells stay selectable there.
send("exit\n")
wait("shell: login shell exited", 120)
time.sleep(1.0)
ensure("sh -c 'echo sh-again=${BASH_VERSION:+bash-again}'", "bash-again")
send("sh -c 'exit 7'\n")
wait("wait status=0x700", 90)
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
check "shell: login shell: bash pid"
check "shell: login shell exited"
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
