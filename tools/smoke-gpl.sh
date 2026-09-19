#!/usr/bin/env bash
# C3 GPL compliance smoke (offline): bash vendored-source integrity, the GPL
# firewall (kernel/base refusal + apps-layer allow list), the `requires` gate
# in the generated menu, the SBOM entry and the kernel/base isolation proof.
#
#   SMOKE_GPL_KEEP=1  keep build/smoke-gpl for inspection
#
# Isolation proof (phase e): the x86_64 kernel is (re)built while the vendored
# bash tree is hashed and timestamp-watched. It proves Cargo's workspace has
# no edge into apps/, the build does not write the tree, and the kernel ELF
# contains no bash symbols or app paths. A silent read cannot be observed, but
# there is no build-graph edge or ELF reference through which one could carry
# GPL code into the kernel or base libraries.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
APPCTL="$ROOT/tools/appctl/appctl.py"
WORK="$ROOT/build/smoke-gpl"
BASH_DIR="$ROOT/apps/bash"
TARBALL="$BASH_DIR/src/bash-5.3.tar.gz"
FRAGMENT="$ROOT/config/apps/bash.kconfig"
ELF="$ROOT/target/x86_64-unknown-none/release/fantuan-kernel"

rm -rf "$WORK"
mkdir -p "$WORK"

fail() { echo "SMOKE FAIL (gpl): $*"; exit 1; }
ok() { echo "SMOKE PASS (gpl: $*)"; }

tree_hash() {
  python3 - "$1" <<'PY'
import sys
sys.path.insert(0, "tools/appctl")
from app_util import tree_sha256
print(tree_sha256(sys.argv[1]))
PY
}

echo "[a] vendored source integrity..."
[ -f "$TARBALL" ] || fail "vendored tarball missing: $TARBALL"
[ -f "$BASH_DIR/COPYING" ] || fail "COPYING missing: $BASH_DIR/COPYING"
( cd "$BASH_DIR/src" && sha256sum -c --status SHA256SUMS ) \
  || fail "tarball sha256 does not match src/SHA256SUMS"
TARBALL_SHA="$(sha256sum "$TARBALL" | cut -d' ' -f1)"
MANIFEST_SHA="$(python3 - "$BASH_DIR/manifest.toml" <<'PY'
import sys, tomllib
print(tomllib.load(open(sys.argv[1], "rb"))["tarball_sha256"])
PY
)"
[ "$TARBALL_SHA" = "$MANIFEST_SHA" ] \
  || fail "manifest tarball_sha256 != actual tarball sha256"
TAR_COPYING="$(tar -xOzf "$TARBALL" bash-5.3/COPYING | sha256sum | cut -d' ' -f1)"
[ "$TAR_COPYING" = "$(sha256sum "$BASH_DIR/COPYING" | cut -d' ' -f1)" ] \
  || fail "COPYING does not match the tarball's bash-5.3/COPYING"
grep -q "Version 3, 29 June 2007" "$BASH_DIR/COPYING" || fail "COPYING is not GPLv3"
ok "source: tarball sha256 ${TARBALL_SHA:0:12} = SHA256SUMS = manifest; COPYING = GPLv3 from tarball"

echo "[b] GPL firewall..."
python3 "$APPCTL" verify --name bash > "$WORK/verify-kernel.log" 2>&1
if [ "$?" -eq 0 ]; then
  cat "$WORK/verify-kernel.log"
  fail "verify bash must FAIL in the kernel/base layer"
fi
grep -q "gpl=true refused in the kernel/base layer" "$WORK/verify-kernel.log" \
  || { cat "$WORK/verify-kernel.log"; fail "kernel-layer refusal message missing"; }
cat "$WORK/verify-kernel.log"
python3 "$APPCTL" verify --name bash --apps-layer > "$WORK/verify-apps.log" 2>&1 \
  || { cat "$WORK/verify-apps.log"; fail "verify --apps-layer must PASS (bash is in gpl_allow)"; }
cat "$WORK/verify-apps.log"
python3 "$APPCTL" verify > /dev/null 2>&1 \
  && fail "verify (all entries) must fail while bash is vendored"
python3 "$APPCTL" verify --apps-layer > /dev/null 2>&1 \
  || fail "verify --apps-layer (all entries) must pass"
ok "firewall: kernel/base refuses bash, apps layer accepts it via [licensing] gpl_allow"

echo "[c] requires gate in the menu..."
python3 "$APPCTL" menu > "$WORK/menu.log" 2> "$WORK/menu.err" \
  || { cat "$WORK/menu.log" "$WORK/menu.err"; fail "menu"; }
grep -q '^config APP_BASH$' "$FRAGMENT" || fail "bash fragment missing"
grep -q '^    default n$' "$FRAGMENT" || fail "fragment must stay default n"
grep -qi 'unavailable' "$FRAGMENT" || fail "fragment misses the unavailable note"
grep -q 'posix-libc' "$FRAGMENT" || fail "fragment misses the requires note"
grep -q 'menu: bash: unavailable' "$WORK/menu.err" || fail "menu misses the stderr note"
STATE="$(python3 tools/kconfig.py --profile minimal --text \
  | awk '$1 == "CONFIG_APP_BASH" { print $2 }')"
[ "$STATE" = "n" ] || fail "CONFIG_APP_BASH is $STATE, want n while requires is unmet"
python3 tools/kconfig.py --check > "$WORK/kcheck.log" 2>&1 \
  || { cat "$WORK/kcheck.log"; fail "tools/kconfig.py --check"; }
cat config/apps/bash.kconfig
ok "requires: menu emits APP_BASH default n with the posix-libc (M14) note"

echo "[d] SBOM..."
python3 "$APPCTL" sbom > "$WORK/sbom.json" 2> "$WORK/sbom.err" \
  || { cat "$WORK/sbom.err"; fail "sbom"; }
python3 - "$WORK/sbom.json" <<'PY' || fail "SBOM entry for bash"
import json, sys
apps = {a["name"]: a for a in json.load(open(sys.argv[1]))["apps"]}
bash = apps.get("bash")
assert bash is not None, sorted(apps)
assert bash["gpl"] is True, bash
assert bash["license"] == "GPL-3.0-or-later", bash
assert bash["version"] == "5.3", bash
print("  sbom: bash", bash["version"], bash["license"], "gpl=true")
PY
ok "sbom: bash entry present with gpl=true and GPL-3.0-or-later"

echo "[e] kernel/base isolation..."
BEFORE="$(tree_hash "$BASH_DIR")"
START="$(date +%s)"
echo "  x86_64 kernel build (incremental), watching apps/bash mtimes and tree hash..."
cargo build -p fantuan-kernel --target x86_64-unknown-none --release \
  > "$WORK/kernel-build.log" 2>&1 || { tail -20 "$WORK/kernel-build.log"; fail "x86_64 kernel build"; }
[ -f "$ELF" ] || fail "kernel ELF not found: $ELF"
AFTER="$(tree_hash "$BASH_DIR")"
[ "$BEFORE" = "$AFTER" ] || fail "kernel build modified apps/bash (tree sha changed)"
TOUCHED="$(find "$BASH_DIR" -newermt "@$START" -print -quit)"
[ -z "$TOUCHED" ] || fail "kernel build wrote $TOUCHED"
python3 - <<'PY' || fail "cargo workspace has an edge into apps/"
import json, subprocess
meta = json.loads(subprocess.run(
    ["cargo", "metadata", "--format-version", "1", "--no-deps"],
    capture_output=True, text=True, check=True).stdout)
bad = [p["manifest_path"] for p in meta["packages"] if "/apps/" in p["manifest_path"]]
assert not bad, bad
print(f"  cargo: {len(meta['packages'])} workspace packages, none under apps/")
PY
if nm "$ELF" 2>/dev/null | grep -qi bash; then
  nm "$ELF" | grep -i bash | head -5
  fail "kernel ELF has bash symbols"
fi
if strings -a "$ELF" | grep -E 'apps/bash|CONFIG_APP_BASH|/usr/src/bash|bash-5\.3' \
     > "$WORK/elf-bash.txt"; then
  cat "$WORK/elf-bash.txt"
  fail "kernel ELF references bash app artifacts"
fi
ok "isolation: apps/bash untouched, no cargo edge, no bash symbols/paths in the x86_64 ELF"

if [ "${SMOKE_GPL_KEEP:-0}" = "1" ]; then
  echo "kept $WORK"
else
  rm -rf "$WORK"
fi
ok "compliance gate (source + firewall + requires menu + sbom + kernel isolation)"
exit 0
