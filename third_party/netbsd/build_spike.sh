#!/usr/bin/env bash
# build_spike.sh - R1 feasibility spike for the imported NetBSD slice.
#
# Compiles every imported .c file as a freestanding x86_64-unknown-none
# object against the adaptation shims in third_party/netbsd/shim/include,
# prints an OK/FAIL table with the first root cause for each failure, then
# links the OK objects with a tiny _start stub and lists every unresolved
# NetBSD kernel service.  Partial success is the expected R1 outcome; the
# unresolved list is the R2 work order.
#
# Output goes to build/netbsd-spike/ (gitignored).  The kernel trees and the
# imported sources are never modified.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DEST="$ROOT/third_party/netbsd"
SHIM="$DEST/shim/include"
OUT="$ROOT/build/netbsd-spike"
CLANG="${CLANG:-clang}"
TARGET="x86_64-unknown-none"
RES_INC="$("$CLANG" -print-resource-dir)/include"

# Same set as tools/import_netbsd.sh (the compile spike is the record of why
# this slice is the closure).
CSRCS="
    sys/kern/subr_evcnt.c
    sys/kern/kern_mutex.c
    sys/kern/kern_condvar.c
    sys/kern/kern_rwlock.c
    sys/kern/kern_lock.c
    sys/kern/kern_timeout.c
    sys/kern/subr_psref.c
    sys/kern/subr_pool.c
    sys/kern/uipc_mbuf.c"

CFLAGS="--target=$TARGET -ffreestanding -nostdinc -isystem $RES_INC \
    -mno-red-zone -fno-stack-protector -fno-builtin -fno-pic -fno-pie \
    -D_KERNEL -I $SHIM -I $DEST/sys -I $DEST/common/include"

mkdir -p "$OUT"
cat > "$OUT/start.c" <<'EOF'
/* tiny freestanding entry point for the spike link (generated here). */
void _start(void)
{
    for (;;) {
    }
}
EOF

classify() {
    local err="$1"
    local miss first
    miss="$(sed -n "s/.*fatal error: '\([^']*\)' file not found.*/\1/p" "$err" | head -1)"
    if [ -n "$miss" ]; then
        case "$miss" in
            machine/*|x86/*) echo "machine-specific dependency: $miss" ;;
            *)               echo "missing header: $miss" ;;
        esac
        return
    fi
    first="$(grep -m1 'error:' "$err" || true)"
    echo "compile error: ${first#*error: }"
}

ok=0
fail=0
declare -a okobjs=()
printf '%-28s %-6s %s\n' "FILE" "RESULT" "FIRST ROOT CAUSE"
for src in $CSRCS; do
    base="$(basename "$src" .c)"
    obj="$OUT/$base.o"
    if $CLANG $CFLAGS -c "$DEST/$src" -o "$obj" 2> "$OUT/$base.err"; then
        printf '%-28s %-6s\n' "$src" "OK"
        okobjs+=("$obj")
        ok=$((ok + 1))
    else
        printf '%-28s %-6s %s\n' "$src" "FAIL" "$(classify "$OUT/$base.err")"
        fail=$((fail + 1))
    fi
done

echo
echo "compile summary: OK=$ok FAIL=$fail"

if [ "$ok" -eq 0 ]; then
    echo "no object compiled; skipping link"
    exit 1
fi

$CLANG $CFLAGS -c "$OUT/start.c" -o "$OUT/start.o"
echo
echo "linking ${ok} OK object(s) with the _start stub"
set +e
$CLANG --target="$TARGET" -nostdlib -no-pie -Wl,-e,_start \
    "$OUT/start.o" "${okobjs[@]}" -o "$OUT/spike.elf" 2> "$OUT/link.err"
link_rc=$?
set -e

# GNU ld: "undefined reference to `sym'"; lld: "undefined symbol: sym".
grep -oE "undefined reference to .[A-Za-z0-9_]+" "$OUT/link.err" |
    sed "s/.*\`//" > "$OUT/undefined.txt" || true
grep -oE "undefined symbol: [A-Za-z0-9_]+" "$OUT/link.err" |
    sed 's/undefined symbol: //' >> "$OUT/undefined.txt" || true
sort -u -o "$OUT/undefined.txt" "$OUT/undefined.txt"
gap="$(grep -c . "$OUT/undefined.txt" || true)"

if [ "$link_rc" -eq 0 ]; then
    echo "link: OK ($OUT/spike.elf)"
else
    echo "link: unresolved NetBSD services = $gap (expected in R1)"
    echo "missing-service gap list (sorted):"
    sed 's/^/    /' "$OUT/undefined.txt"
fi
