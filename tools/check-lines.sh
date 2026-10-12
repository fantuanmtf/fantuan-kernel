#!/usr/bin/env bash
# 300-line rule ratchet (docs/DEVELOPMENT.md §1.3): no source file under
# kernel*, abi, user, boot or drivers may exceed 300 lines. third_party is
# exempt (imported sources keep their shape); tools/, libc-fantuan/ and docs/
# are outside the rule's scope as docs/OPERATIONS.md §5 states it.
#
# Ten files were over the line when this gate was added (2026-10), so it cannot simply
# fail: it fails on **new** offenders and on stale budget entries, which means
# the list can only shrink. Lower a number here when you split a file, and
# delete the line when the file drops under the limit.
#
#   tools/check-lines.sh            # check
#   tools/check-lines.sh --list     # print every in-scope file over the limit
set -uo pipefail
export LC_ALL=C   # wc's summary line is localized otherwise, and it is not a file
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

LIMIT=300
BUDGET="$ROOT/tools/line-budget.txt"

# In-scope trees (the rule's scope as docs/OPERATIONS.md §5 words it). Listed
# explicitly rather than globbed so the scope is visible in the code.
in_scope() {
  local dirs=() d
  for d in kernel kernel-core kernel-i686 kernel-net kernel-riscv kernel-aarch64 \
           abi user boot boot-bios drivers; do
    [ -d "$d" ] && dirs+=("$d")
  done
  find "${dirs[@]}" -type f \( -name '*.rs' -o -name '*.c' -o -name '*.h' \
       -o -name '*.S' -o -name '*.asm' \) -print
}

if [ "${1:-}" = "--list" ]; then
  in_scope | xargs wc -l | awk -v lim="$LIMIT" '$1 > lim && $2 != "total" { print $1, $2 }' | sort -rn
  exit 0
fi

list_over() { in_scope | xargs wc -l | awk -v lim="$LIMIT" '$1 > lim && $2 != "total" { print $2 }' | sort; }

FAIL=0
OVER="$(list_over)"
# New offenders: over the limit but not in the budget.
NEW="$(comm -23 <(printf '%s\n' $OVER) <(grep -v '^#' "$BUDGET" 2>/dev/null | awk 'NF >= 1 { print $1 }' | sort))"
if [ -n "$NEW" ]; then
  echo "check-lines: FAIL — over $LIMIT lines and not in tools/line-budget.txt:"
  for f in $NEW; do echo "  $(wc -l < "$f") $f"; done
  echo "  split the file, or record it in the budget with a reason."
  FAIL=1
fi
# Stale entries: in the budget but no longer over the limit (or gone).
while read -r path _; do
  [ -z "$path" ] && continue
  [ -f "$path" ] || { echo "check-lines: FAIL — budget entry $path does not exist"; FAIL=1; continue; }
  lines="$(wc -l < "$path")"
  if [ "$lines" -le "$LIMIT" ]; then
    echo "check-lines: FAIL — $path is $lines lines, drop it from the budget"
    FAIL=1
  fi
done < <(grep -v '^#' "$BUDGET" 2>/dev/null)

if [ "$FAIL" = "0" ]; then
  n="$(printf '%s\n' $OVER | grep -c . || true)"
  echo "check-lines: OK ($n file(s) over $LIMIT lines, all budgeted — the list only shrinks)"
fi
exit "$FAIL"
