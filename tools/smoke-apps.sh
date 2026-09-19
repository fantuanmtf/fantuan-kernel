#!/usr/bin/env bash
# C2 apps smoke (offline): builds a fixture catalog in build/smoke-apps and
# exercises appctl end to end - add/sync, the lock sha256, verify (GPL firewall),
# menu + tools/kconfig.py merge, sbom and remove. No network.
#
#   SMOKE_APPS_KEEP=1  keep build/smoke-apps for inspection
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
APPCTL="$ROOT/tools/appctl/appctl.py"
WORK="$ROOT/build/smoke-apps"
FIX="$WORK/catalog"
REPO="$WORK/repo"
FRAGMENT="$ROOT/config/apps/smoke-bsdhello.kconfig"
CATALOG="$FIX/apps-catalog.toml"

rm -rf "$WORK"
mkdir -p "$FIX/apps/bsdhello/patches" "$FIX/apps/bsdhello/src" \
         "$FIX/apps/gpltool/patches" "$FIX/apps/gpltool/src" \
         "$REPO/kernel-core/src"
cp "$ROOT/kernel-core/src/syscall.rs" "$REPO/kernel-core/src/syscall.rs"

cleanup() {
  rm -f "$FRAGMENT"
  rmdir --ignore-fail-on-non-empty "$ROOT/config/apps" 2>/dev/null || true
  if [ "${SMOKE_APPS_KEEP:-0}" = "1" ]; then
    echo "kept $WORK"
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT

fail() { echo "SMOKE FAIL (apps): $*"; exit 1; }
ok() { echo "SMOKE PASS (apps: $*)"; }

lock_field() { # name field
  python3 - "$REPO/apps.lock" "$1" "$2" <<'PY'
import sys, tomllib
data = tomllib.load(open(sys.argv[1], "rb"))
for app in data.get("app", []):
    if app["name"] == sys.argv[2]:
        print(app[sys.argv[3]])
        break
PY
}

tree_sha() { # dir
  (cd "$1" && find . -type f -printf '%P\n' | LC_ALL=C sort | while IFS= read -r f; do
    printf '%s %s\n' "$f" "$(sha256sum "$f" | cut -d' ' -f1)"
  done | sha256sum | cut -d' ' -f1)
}

appctl() { python3 "$APPCTL" --root "$REPO" --catalog "$CATALOG" "$@"; }

write_fixture() {
cat > "$FIX/apps/bsdhello/manifest.toml" <<'EOF'
name = "bsdhello"
version = "1.0"
license = "BSD-2-Clause"
upstream = "https://example.invalid/bsdhello.git"
rev = "0000000000000000000000000000000000000000"
abi_min = 1
build = "custom"
deps = []
patches = []
description = "BSD fixture app with a passthrough build"
gpl = false
EOF
cat > "$FIX/apps/bsdhello/README.md" <<'EOF'
# bsdhello

Fixture app: `src/build.sh` prints a marker.
EOF
cat > "$FIX/apps/bsdhello/src/build.sh" <<'EOF'
#!/bin/sh
echo "bsdhello: build ok"
EOF
printf 'hello from the fixture\n' > "$FIX/apps/bsdhello/src/hello.txt"
: > "$FIX/apps/bsdhello/patches/.gitkeep"

cat > "$FIX/apps/gpltool/manifest.toml" <<'EOF'
name = "gpltool"
version = "2.1"
license = "GPL-2.0-only"
upstream = "https://example.invalid/gpltool.git"
rev = "0000000000000000000000000000000000000000"
abi_min = 1
build = "custom"
deps = []
patches = []
description = "GPL fixture app (must be refused in the kernel layer)"
gpl = true
EOF
cat > "$FIX/apps/gpltool/README.md" <<'EOF'
# gpltool

Fixture GPL app; never linked into the kernel or base.
EOF
printf 'gpl fixture\n' > "$FIX/apps/gpltool/src/tool.txt"
: > "$FIX/apps/gpltool/patches/.gitkeep"

cat > "$CATALOG" <<'EOF'
version = 1
[default]
source = "fixture"
[[source]]
name = "fixture"
kind = "path"
path = "."
apps = "apps"
enabled = true
[licensing]
gpl_allow = []
EOF
cat > "$FIX/apps-catalog-gpl.toml" <<'EOF'
version = 1
[default]
source = "fixture"
[[source]]
name = "fixture"
kind = "path"
path = "."
apps = "apps"
enabled = true
[licensing]
gpl_allow = ["gpltool"]
EOF
}

write_fixture
git -C "$FIX" init -q
git -C "$FIX" add -A
git -C "$FIX" -c user.name=smoke -c user.email=smoke@example.invalid commit -qm fixture
FIX_REV="$(git -C "$FIX" rev-parse HEAD)"

echo "[add] bsdhello from the fixture catalog..."
appctl add bsdhello > "$WORK/add.log" 2>&1 || { cat "$WORK/add.log"; fail "add bsdhello"; }
[ -f "$REPO/apps/bsdhello/manifest.toml" ] || fail "vendored manifest missing"
[ -f "$REPO/apps/bsdhello/src/hello.txt" ] || fail "vendored src missing"
[ -d "$REPO/apps/bsdhello/patches" ] || fail "vendored patches/ missing"
[ "$(lock_field bsdhello rev)" = "$FIX_REV" ] || fail "lock rev is not the fixture git HEAD"
LOCK_SHA="$(lock_field bsdhello sha256)"
[ "$LOCK_SHA" = "$(tree_sha "$REPO/apps/bsdhello")" ] || fail "lock sha256 != recomputed tree hash"
ok "add: tree + lock entry pinned (rev ${FIX_REV:0:12}, sha256 ${LOCK_SHA:0:12})"

echo "[add] gpltool (vendoring is allowed; verify enforces the firewall)..."
appctl add gpltool > "$WORK/add-gpl.log" 2>&1 || { cat "$WORK/add-gpl.log"; fail "add gpltool"; }
ok "add: gpltool vendored and pinned"

echo "[verify] kernel/base layer..."
appctl verify --name bsdhello > "$WORK/verify-bsd.log" 2>&1 \
  || { cat "$WORK/verify-bsd.log"; fail "verify bsdhello should pass"; }
if appctl verify --name gpltool > "$WORK/verify-gpl.log" 2>&1; then
  fail "verify gpltool should fail in the kernel/base layer"
fi
grep -q "kernel/base layer" "$WORK/verify-gpl.log" || fail "missing kernel-layer refusal reason"
echo "  $(grep -m1 'gpl=true' "$WORK/verify-gpl.log" | sed 's/^ *//')"
ok "verify: bsdhello passes, gpltool refused (gpl=true) in the kernel layer"

echo "[verify] apps layer allow list..."
if appctl verify --name gpltool --apps-layer > "$WORK/verify-gpl-allow.log" 2>&1; then
  fail "gpltool is not in [licensing] gpl_allow and must not pass"
fi
python3 "$APPCTL" --root "$REPO" --catalog "$FIX/apps-catalog-gpl.toml" \
  verify --name gpltool --apps-layer > "$WORK/verify-gpl-listed.log" 2>&1 \
  || { cat "$WORK/verify-gpl-listed.log"; fail "listed GPL app should pass in the apps layer"; }
ok "verify: GPL accepted only when listed in the apps-layer allow list"

echo "[sync] explicit --from path..."
appctl sync --from "$FIX" --name bsdhello > "$WORK/sync.log" 2>&1 \
  || { cat "$WORK/sync.log"; fail "sync --from"; }
[ "$(lock_field bsdhello source)" = "path:$FIX" ] || fail "lock source not recorded"
[ "$(lock_field bsdhello sha256)" = "$(tree_sha "$REPO/apps/bsdhello")" ] \
  || fail "sha256 after sync"
ok "sync: source and sha256 refreshed from --from $FIX"

echo "[verify] corrupted tree..."
printf 'corruption\n' >> "$REPO/apps/bsdhello/src/hello.txt"
if appctl verify --name bsdhello > "$WORK/verify-corrupt.log" 2>&1; then
  fail "verify must fail on a corrupted tree"
fi
grep -q "sha256 mismatch" "$WORK/verify-corrupt.log" || fail "missing sha256 mismatch"
appctl sync --from "$FIX" --name bsdhello > /dev/null 2>&1 || fail "re-sync"
appctl verify --name bsdhello > /dev/null 2>&1 || fail "verify after re-sync"
ok "verify: corrupt tree fails (sha256 mismatch), re-sync restores"

echo "[menu] Kconfig fragments..."
appctl menu > "$WORK/menu.log" 2>&1 || { cat "$WORK/menu.log"; fail "menu"; }
[ -f "$REPO/config/apps/bsdhello.kconfig" ] || fail "bsdhello fragment missing"
[ -f "$REPO/config/apps/gpltool.kconfig" ] && fail "GPL fragment must be skipped in the kernel layer"
grep -q "config APP_BSDHELLO" "$REPO/config/apps/bsdhello.kconfig" || fail "fragment symbol"
mkdir -p "$ROOT/config/apps"
cp "$REPO/config/apps/bsdhello.kconfig" "$FRAGMENT"
python3 tools/kconfig.py --check > "$WORK/kcheck.log" 2>&1 \
  || { cat "$WORK/kcheck.log"; fail "tools/kconfig.py --check rejected the fragment"; }
python3 tools/kconfig.py --text | grep -q "CONFIG_APP_BSDHELLO" \
  || fail "kconfig.py did not merge config/apps/*.kconfig"
echo "  $(python3 tools/kconfig.py --check)"
ok "menu: fragment generated and merged by tools/kconfig.py --check"

echo "[sbom] JSON with both entries..."
appctl sbom > "$WORK/sbom.json" 2> "$WORK/sbom.log" || { cat "$WORK/sbom.log"; fail "sbom"; }
python3 - "$WORK/sbom.json" <<'PY' || fail "invalid SBOM"
import json, sys
doc = json.load(open(sys.argv[1]))
names = {app["name"] for app in doc["apps"]}
assert names == {"bsdhello", "gpltool"}, names
for app in doc["apps"]:
    for key in ("name", "version", "license", "gpl", "source", "rev", "sha256"):
        assert key in app, (app["name"], key)
    assert app["gpl"] is (app["name"] == "gpltool")
print("  apps:", ", ".join(sorted(names)))
PY
ok "sbom: valid JSON with both entries"

echo "[remove] tree + lock entry + fragment..."
appctl remove bsdhello > "$WORK/remove.log" 2>&1 || { cat "$WORK/remove.log"; fail "remove bsdhello"; }
[ ! -e "$REPO/apps/bsdhello" ] || fail "bsdhello tree not removed"
[ -z "$(lock_field bsdhello name)" ] || fail "bsdhello lock entry not removed"
[ ! -e "$REPO/config/apps/bsdhello.kconfig" ] || fail "bsdhello fragment not removed"
appctl remove gpltool > /dev/null 2>&1 || fail "remove gpltool"
python3 - "$REPO/apps.lock" <<'PY' || fail "lock not empty after remove"
import sys, tomllib
assert tomllib.load(open(sys.argv[1], "rb")).get("app", []) == []
PY
ok "remove: tree, lock entry and menu fragment cleaned up"

ok "offline gate (add/sync/verify/menu/sbom/remove + GPL firewall)"
exit 0
