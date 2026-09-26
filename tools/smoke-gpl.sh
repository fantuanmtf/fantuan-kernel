#!/usr/bin/env bash
# GPL compliance smoke (offline): main carries no GPL source, the source pins
# agree, the firewall (kernel/base refusal + apps-layer allow list), the
# `requires` gate in the generated menu, the SBOM entry, the fantuan-apps
# source bundle and the kernel/base isolation proof.
#
#   SMOKE_GPL_KEEP=1  keep build/smoke-gpl for inspection
#
# Isolation proof (phase [e]): the x86_64 kernel is (re)built while the bash
# metadata tree is hashed and timestamp-watched. It proves Cargo's workspace
# has no edge into apps/, the build does not write that tree, and bash is
# **not linked** (no bash code symbols, no app paths in the kernel ELF).
# It does NOT prove the image is free of bash: the stripped bash program is
# embedded as an opaque byte blob (include_bytes!, kernel/build.rs), so the
# kernel image *is* a distribution of bash and the GPLv3 source provision in
# THIRD_PARTY.md applies to it. Phase [e] asserts both halves explicitly —
# the absence of a link and the presence of the recorded artifact.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
APPCTL="$ROOT/tools/appctl/appctl.py"
WORK="$ROOT/build/smoke-gpl"
BASH_DIR="$ROOT/apps/bash"
FRAGMENT="$ROOT/config/apps/bash.kconfig"
ELF="$ROOT/target/x86_64-unknown-none/release/fantuan-kernel"
MARKER="$ROOT/kernel/bash_program.sha256"
BLOB="$ROOT/kernel/bash_program.bin"
BRANCH_REF="${SMOKE_GPL_BRANCH:-origin/fantuan-apps}"

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

manifest_field() { # field
  python3 - "$BASH_DIR/manifest.toml" "$1" <<'PY'
import sys, tomllib
print(tomllib.load(open(sys.argv[1], "rb"))[sys.argv[2]])
PY
}

echo "[a1] main carries no GPL source..."
ARCHIVES="$(git ls-tree -r --name-only HEAD apps/bash \
  | grep -E '\.(tar|tar\.gz|tgz|tar\.bz2|tar\.xz)$' || true)"
[ -z "$ARCHIVES" ] || fail "main tracks a bash archive again: $ARCHIVES"
SRC_FILES="$(git ls-tree -r --name-only HEAD apps/bash/src | sed 's#.*/##' | sort | tr '\n' ' ')"
[ "$SRC_FILES" = "SHA256SUMS SOURCE " ] \
  || fail "apps/bash/src tracks [$SRC_FILES] (want only SHA256SUMS + SOURCE)"
TRACKED_BYTES="$(git ls-tree -r -l HEAD apps/bash | awk '{s += $4} END {print s + 0}')"
[ "$TRACKED_BYTES" -lt 262144 ] \
  || fail "apps/bash tracks $TRACKED_BYTES bytes (metadata only, want < 256 KiB)"
# dash (BSD-3) and mbedtls (Apache-2.0) legitimately keep their tarballs, so
# this deliberately checks the bash subtree only.
if [ -e "$BASH_DIR/src/bash-5.3.tar.gz" ]; then
  git check-ignore -q "$BASH_DIR/src/bash-5.3.tar.gz" \
    || fail "a local bash tarball would be tracked (not gitignored)"
fi
ok "tip tree: apps/bash is metadata only ($TRACKED_BYTES bytes tracked, no archive)"

echo "[a2] the three source pins agree (no tarball needed)..."
SUMS_SHA="$(awk 'NF == 2 {print $1}' "$BASH_DIR/src/SHA256SUMS" | head -1)"
MANIFEST_SHA="$(manifest_field tarball_sha256)"
SOURCE_SHA="$(sed -n 's/^sha256:[[:space:]]*//p' "$BASH_DIR/src/SOURCE" | head -1)"
[ -n "$SUMS_SHA" ] || fail "src/SHA256SUMS has no hash line"
[ "$SUMS_SHA" = "$MANIFEST_SHA" ] \
  || fail "SHA256SUMS ($SUMS_SHA) != manifest.tarball_sha256 ($MANIFEST_SHA)"
[ "$SUMS_SHA" = "$SOURCE_SHA" ] \
  || fail "SHA256SUMS ($SUMS_SHA) != src/SOURCE ($SOURCE_SHA)"
PROVISION="$(manifest_field source_provision)"
case "$PROVISION" in
  *"$SUMS_SHA"*) : ;;
  *) fail "manifest.source_provision does not name the pinned hash" ;;
esac
grep -q "fantuan-apps" <<<"$PROVISION" || fail "source_provision does not name the mirror"
ok "source: pins agree on ${SUMS_SHA:0:12}; source_provision names the mirror"

echo "[a3] the tarball itself (when present locally)..."
TARBALL=""
for candidate in "$ROOT/build/cache/bash-5.3.tar.gz" "$BASH_DIR/src/bash-5.3.tar.gz"; do
  [ -f "$candidate" ] && TARBALL="$candidate" && break
done
if [ -n "$TARBALL" ]; then
  [ "$(sha256sum "$TARBALL" | cut -d' ' -f1)" = "$SUMS_SHA" ] \
    || fail "$TARBALL does not match the pinned sha256"
  TAR_COPYING="$(tar -xOzf "$TARBALL" bash-5.3/COPYING | sha256sum | cut -d' ' -f1)"
  [ "$TAR_COPYING" = "$(sha256sum "$BASH_DIR/COPYING" | cut -d' ' -f1)" ] \
    || fail "COPYING does not match the tarball's bash-5.3/COPYING"
  grep -q "Version 3, 29 June 2007" "$BASH_DIR/COPYING" || fail "COPYING is not GPLv3"
  ok "tarball: $(basename "$(dirname "$TARBALL")")/$(basename "$TARBALL") matches the pin; COPYING = GPLv3 from it"
else
  echo "  SKIP: no local tarball (run tools/fetch-bash-src.sh to prove the archive)"
fi

echo "[a4] the fantuan-apps mirror carries the pinned bytes..."
if git rev-parse --verify -q "$BRANCH_REF:apps/bash/src/bash-5.3.tar.gz" >/dev/null; then
  MIRROR_SHA="$(git cat-file blob "$BRANCH_REF:apps/bash/src/bash-5.3.tar.gz" | sha256sum | cut -d' ' -f1)"
  [ "$MIRROR_SHA" = "$SUMS_SHA" ] || fail "$BRANCH_REF carries $MIRROR_SHA, want $SUMS_SHA"
  PATCH="apps/bash/patches/0001-netopen-no-network-decls.patch"
  [ "$(git rev-parse "$BRANCH_REF:$PATCH")" = "$(git rev-parse "HEAD:$PATCH")" ] \
    || fail "$BRANCH_REF's port patch differs from main's"
  ok "mirror: $BRANCH_REF bytes match the pin; the port patch matches main"
else
  echo "  SKIP: $BRANCH_REF not fetched (git fetch origin fantuan-apps)"
fi

echo "[b] GPL firewall..."
python3 "$APPCTL" verify --name bash > "$WORK/verify-kernel.log" 2>&1
if [ "$?" -eq 0 ]; then
  cat "$WORK/verify-kernel.log"
  fail "verify bash must FAIL in the kernel/base layer"
fi
grep -q "gpl=true refused in the kernel/base layer" "$WORK/verify-kernel.log" \
  || { cat "$WORK/verify-kernel.log"; fail "kernel-layer refusal message missing"; }
grep -q "sha256 mismatch" "$WORK/verify-kernel.log" \
  && fail "verify failed on the tree hash, not on the firewall (run appctl relock)"
cat "$WORK/verify-kernel.log"
python3 "$APPCTL" verify --name bash --apps-layer > "$WORK/verify-apps.log" 2>&1 \
  || { cat "$WORK/verify-apps.log"; fail "verify --apps-layer must PASS (bash is in gpl_allow)"; }
cat "$WORK/verify-apps.log"
LOCK_SHA="$(python3 - "$ROOT/apps.lock" <<'PY'
import sys, tomllib
entries = tomllib.load(open(sys.argv[1], "rb"))["app"]
print([e for e in entries if e["name"] == "bash"][0]["sha256"])
PY
)"
[ "$LOCK_SHA" = "$(tree_hash "$BASH_DIR")" ] \
  || fail "apps.lock bash sha256 is stale (want $(tree_hash "$BASH_DIR"))"
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
python3 - "$WORK/sbom.json" "$PROVISION" <<'PY' || fail "SBOM entry for bash"
import json, sys
apps = {a["name"]: a for a in json.load(open(sys.argv[1]))["apps"]}
bash = apps.get("bash")
assert bash is not None, sorted(apps)
assert bash["gpl"] is True, bash
assert bash["license"] == "GPL-3.0-or-later", bash
assert bash["version"] == "5.3", bash
assert bash["source"] == "upstream", bash
assert "fantuan-apps" in sys.argv[2], sys.argv[2]
print("  sbom: bash", bash["version"], bash["license"], "gpl=true, source provisioned via the mirror")
PY
ok "sbom: bash entry present with gpl=true and GPL-3.0-or-later"

echo "[e] kernel/base isolation (and what the image actually carries)..."
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
# No bash *code* may be linked. BASH_ELF (the Rust data static holding the
# blob) is expected and must not trip this, hence the anchored pattern.
if nm "$ELF" 2>/dev/null | awk 'NF >= 2 { print $NF }' \
     | grep -qiE '^_?(bash|main_bash|bash_main|shell_main)$'; then
  nm "$ELF" | grep -iE '^_?(bash|main_bash|bash_main|shell_main)$' | head -5
  fail "kernel ELF has linked bash code symbols"
fi
if strings -a "$ELF" | grep -E 'apps/bash|CONFIG_APP_BASH|/usr/src/bash|bash-5\.3' \
     > "$WORK/elf-bash.txt"; then
  cat "$WORK/elf-bash.txt"
  fail "kernel ELF references bash app artifacts"
fi
if [ -f "$BLOB" ] && [ -f "$MARKER" ]; then
  SHELL_SHA="$(cat "$MARKER")"
  [ "$(sha256sum "$BLOB" | cut -d' ' -f1)" = "$SHELL_SHA" ] \
    || fail "kernel/bash_program.sha256 does not match kernel/bash_program.bin"
  # No `grep -q` in the pipe: under `set -o pipefail` an early exit SIGPIPEs
  # `strings` and the successful case would read as a failure.
  strings -a "$ELF" > "$WORK/elf-strings.txt"
  grep -qF "$SHELL_SHA" "$WORK/elf-strings.txt" \
    || fail "kernel ELF does not carry the embedded shell marker $SHELL_SHA"
  grep -q "$SHELL_SHA" "$BASH_DIR/port/README.md" \
    || fail "the embedded shell hash is not the one recorded in port/README.md"
  ok "not linked, but the image embeds the shell: $BLOB sha256 ${SHELL_SHA:0:12} is in the ELF (GPLv3 source provision applies)"
else
  echo "  SKIP: no embedded shell artifact (the default tools/build.sh produces it, plus $MARKER)"
fi

if [ "${SMOKE_GPL_KEEP:-0}" = "1" ]; then
  echo "kept $WORK"
else
  rm -rf "$WORK"
fi
ok "compliance gate (tip tree + pins + mirror + firewall + requires menu + sbom + kernel isolation)"
exit 0
