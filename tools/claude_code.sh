#!/usr/bin/env bash
# Headless dispatch of Claude Code as an implementation subagent.
#
# Claude Code (the `claude` CLI) carries its own agent harness (tools, agents,
# skills, MCP) and its model routing comes from ~/.claude/settings.json
# (currently ANTHROPIC_BASE_URL pointed at DeepSeek; switch it to Anthropic or
# any Claude-capable router and the same call yields Claude with no change).
#
# Usage:
#   tools/claude_code.sh -f prompt.md [--model M] [--timeout S] [--max-turns N]
#   tools/claude_code.sh [--safe] "one-shot prompt"
#   tools/claude_code.sh -f prompt.md --append-system "extra system text"
#
# --safe           acceptEdits instead of bypassPermissions (asks for bash)
# --model M        override the model for this run (e.g. deepseek-flash)
# Output: build/claude-code/<timestamp>.json (raw) + .err; a summary is printed.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROMPT_FILE=""
PROMPT=""
MODEL=""
TIMEOUT="${CC_TIMEOUT:-1800}"
MAX_TURNS="${CC_MAX_TURNS:-50}"
SYSTEM=""
SAFE=0

while [ $# -gt 0 ]; do
  case "$1" in
    -f) PROMPT_FILE="$2"; shift 2 ;;
    --model) MODEL="$2"; shift 2 ;;
    --timeout) TIMEOUT="$2"; shift 2 ;;
    --max-turns) MAX_TURNS="$2"; shift 2 ;;
    --append-system) SYSTEM="$2"; shift 2 ;;
    --safe) SAFE=1; shift ;;
    -*) echo "claude_code: unknown flag $1" >&2; exit 2 ;;
    *) PROMPT="$1"; shift ;;
  esac
done
[ -n "$PROMPT_FILE" ] && PROMPT="$(cat "$PROMPT_FILE")"
[ -n "$PROMPT" ] || { echo "claude_code: empty prompt" >&2; exit 2; }

mkdir -p "$ROOT/build/claude-code"
OUT="$ROOT/build/claude-code/$(date +%Y%m%d-%H%M%S).json"
FLAGS=(--print --output-format json --max-turns "$MAX_TURNS")
if [ "$SAFE" = 1 ]; then FLAGS+=(--permission-mode acceptEdits); else FLAGS+=(--permission-mode bypassPermissions); fi
[ -n "$MODEL" ] && FLAGS+=(--model "$MODEL")
[ -n "$SYSTEM" ] && FLAGS+=(--append-system-prompt "$SYSTEM")

cd "$ROOT"
set +e
timeout "$TIMEOUT" claude "${FLAGS[@]}" "$PROMPT" > "$OUT" 2> "${OUT%.json}.err"
rc=$?
set -e
echo "[claude_code] exit=$rc raw=$OUT"
python3 - "$OUT" <<'PY' 2>/dev/null || tail -c 600 "${OUT%.json}.err"
import json, sys
d = json.load(open(sys.argv[1]))
print("[claude_code] duration_ms=%s turns=%s cost_usd=%s is_error=%s" % (
    d.get("duration_ms"), d.get("num_turns"), d.get("total_cost_usd"), d.get("is_error")))
print((d.get("result") or "")[-1200:])
PY
exit "$rc"
