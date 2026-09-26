#!/usr/bin/env bash
# Obtain and verify the GNU Bash 5.3 release tarball the default build needs.
#
# The pristine upstream sources are deliberately NOT tracked on main (see
# THIRD_PARTY.md and docs/APPS.md): main keeps only the pins - the upstream URL
# and tarball_sha256 in apps/bash/manifest.toml, mirrored by
# apps/bash/src/SHA256SUMS and apps/bash/src/SOURCE - and this script turns one
# of the sources below into a verified file under build/cache/ (gitignored).
#
#   1. $FANTUAN_BASH_TARBALL          an operator-provided copy
#   2. build/cache/bash-5.3.tar.gz    a previous fetch
#   3. apps/bash/src/                 a tree vendored by tools/appctl
#   4. the pinned upstream URL        network; sha256 + GPG (when gpg exists)
#   5. --from-branch [REF]            git blob from the fantuan-apps mirror
#
# Usage:
#   tools/fetch-bash-src.sh [--from-branch [REF]] [--to-skeleton] [--force]
#
#   --from-branch [REF]  resolve from a git ref (default origin/fantuan-apps);
#                        works offline once the branch has been fetched
#   --to-skeleton        also copy the tarball + .sig into the fantuan-apps
#                        branch skeleton (build/branch-skeletons/fantuan-apps)
#   --force              refetch even when the cache holds a valid file
#
#   FANTUAN_OFFLINE=1            never touch the network; fail with guidance
#   FANTUAN_BASH_TARBALL=<file>  use this file (verified before use)
#
# stdout carries exactly one line: the path of the verified tarball.
# Everything else goes to stderr. Exit 0 on success, 2 on any failure.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

MANIFEST="$ROOT/apps/bash/manifest.toml"
SUMS="$ROOT/apps/bash/src/SHA256SUMS"
SOURCE="$ROOT/apps/bash/src/SOURCE"
CACHE="$ROOT/build/cache"
NAME="bash-5.3.tar.gz"
SIG="$NAME.sig"
KEYRING="$CACHE/gnu-keyring.gpg"
KEYRING_URL="https://ftp.gnu.org/gnu/gnu-keyring.gpg"
SKELETON="$ROOT/build/branch-skeletons/fantuan-apps/apps/bash/src"
REF_DEFAULT="origin/fantuan-apps"
REF="$REF_DEFAULT"
OFFLINE="${FANTUAN_OFFLINE:-0}"

USE_BRANCH=0
TO_SKELETON=0
FORCE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --from-branch) USE_BRANCH=1; [ $# -gt 1 ] && [ "${2#--}" = "$2" ] && { REF="$2"; shift; } ;;
    --to-skeleton) TO_SKELETON=1 ;;
    --force) FORCE=1 ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    *) echo "fetch-bash-src: unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
REF="${FANTUAN_BASH_REF:-$REF}"

say() { echo "[fetch-bash-src] $*" >&2; }
die() {
  echo "fetch-bash-src: $*" >&2
  cat >&2 <<'EOF'
fetch-bash-src: how to get the sources:
  * online (default):   tools/fetch-bash-src.sh
  * from a mirror:      git fetch origin fantuan-apps && tools/fetch-bash-src.sh --from-branch
  * a local copy:       FANTUAN_BASH_TARBALL=/path/bash-5.3.tar.gz tools/fetch-bash-src.sh
  * no shell at all:    tools/kconfig.py --symbol BASH=N   (kernel keeps dash/sh fallback)
                        or FANTUAN_BUILD_BASH=0 tools/build.sh
EOF
  exit 2
}

# Pins: the manifest, SHA256SUMS and SOURCE must agree before anything is used.
read_pins() {
  python3 - "$MANIFEST" "$SUMS" "$SOURCE" <<'PY'
import sys, tomllib
manifest, sums, source = sys.argv[1:4]
m = tomllib.load(open(manifest, "rb"))
url, sha = m.get("upstream", "").strip(), m.get("tarball_sha256", "").strip()
sums_sha = ""
for line in open(sums):
    parts = line.split()
    if len(parts) == 2 and parts[1].endswith(".tar.gz"):
        sums_sha = parts[0].strip()
source_sha = ""
for line in open(source):
    if line.lower().startswith("sha256:"):
        source_sha = line.split(":", 1)[1].strip()
if not (url and sha and sha == sums_sha == source_sha):
    print(f"pin mismatch: manifest={sha} SHA256SUMS={sums_sha} SOURCE={source_sha}",
          file=sys.stderr)
    raise SystemExit(2)
print(url, sha)
PY
}

verify() { # file
  printf '%s  %s\n' "$SHA" "$1" | sha256sum -c --status - 2>/dev/null
}

download() { # url dest
  [ "$OFFLINE" = "1" ] && return 3
  if command -v curl >/dev/null 2>&1; then
    curl -fL --proto '=https' --tlsv1.2 --connect-timeout 20 -o "$2" "$1" 2>/dev/null
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1" 2>/dev/null
  else
    return 4
  fi
}

gpg_verify() { # tarball sig
  command -v gpg >/dev/null 2>&1 || { say "gpg not installed: signature check skipped"; return 0; }
  [ "$OFFLINE" = "1" ] && { say "offline: signature check skipped"; return 0; }
  if [ ! -f "$KEYRING" ]; then
    download "$KEYRING_URL" "$KEYRING.part" || { say "cannot fetch the GNU keyring; signature check skipped"; return 0; }
    mv "$KEYRING.part" "$KEYRING"
  fi
  local out
  out="$(gpg --batch --no-default-keyring --keyring "$KEYRING" \
            --status-fd 1 --verify "$2" "$1" 2>/dev/null)" || {
    say "GPG signature verification FAILED for $1"; return 1; }
  grep -q "GOODSIG .* ${FPR_TAIL}\$" <<<"$out" || {
    say "GPG signature is good but by an unexpected key (want ...${FPR_TAIL})"; return 1; }
  say "GPG signature verified (key ...${FPR_TAIL})"
}

place() { # src -> cache (atomic) ; sets TARBALL
  [ "$FORCE" = "1" ] && rm -f "$CACHE/$NAME"
  mkdir -p "$CACHE"
  if [ "$1" != "$CACHE/$NAME" ]; then
    cp -f "$1" "$CACHE/$NAME.part" && mv -f "$CACHE/$NAME.part" "$CACHE/$NAME" || die "cannot populate $CACHE"
  fi
}

PINS="$(read_pins)" || die "the bash source pins disagree (manifest/SHA256SUMS/SOURCE)"
URL="${PINS% *}"
SHA="${PINS##* }"
FPR_TAIL="$(grep -oE '[0-9A-F]{40}' "$SOURCE" | head -1)"
FPR_TAIL="${FPR_TAIL: -16}"
[ -n "$FPR_TAIL" ] || die "no GPG key fingerprint found in $SOURCE"
mkdir -p "$CACHE"

# 1. operator-provided copy
if [ -n "${FANTUAN_BASH_TARBALL:-}" ]; then
  [ -f "$FANTUAN_BASH_TARBALL" ] || die "FANTUAN_BASH_TARBALL=$FANTUAN_BASH_TARBALL is not a file"
  verify "$FANTUAN_BASH_TARBALL" || die "FANTUAN_BASH_TARBALL does not match sha256 $SHA"
  say "source: operator-provided $FANTUAN_BASH_TARBALL"
  place "$FANTUAN_BASH_TARBALL"

# 2. cache
elif [ "$FORCE" != "1" ] && [ -f "$CACHE/$NAME" ] && verify "$CACHE/$NAME"; then
  say "source: cache $CACHE/$NAME"

# 3. a tree vendored under apps/bash/src/ (tools/appctl sync)
elif [ "$FORCE" != "1" ] && [ -f "$ROOT/apps/bash/src/$NAME" ] && verify "$ROOT/apps/bash/src/$NAME"; then
  say "source: vendored tree apps/bash/src/$NAME"
  place "$ROOT/apps/bash/src/$NAME"

else
  # 4. the pinned upstream release
  if [ "$USE_BRANCH" = "0" ]; then
    [ "$OFFLINE" = "1" ] || say "fetching $URL"
    if download "$URL" "$CACHE/$NAME.part"; then
      verify "$CACHE/$NAME.part" || die "downloaded tarball does not match sha256 $SHA"
      mv -f "$CACHE/$NAME.part" "$CACHE/$NAME"
      say "source: upstream $URL"
      if download "$URL.sig" "$CACHE/$SIG.part"; then
        mv -f "$CACHE/$SIG.part" "$CACHE/$SIG"
        gpg_verify "$CACHE/$NAME" "$CACHE/$SIG" || die "refusing an unverified bash tarball"
      else
        say "detached signature unavailable; sha256 pin still enforced"
      fi
    else
      say "upstream fetch unavailable (offline, or curl/wget missing)"
    fi
  fi
  # 5. the fantuan-apps mirror
  if [ ! -f "$CACHE/$NAME" ] && [ "$USE_BRANCH" = "1" ]; then
    if git rev-parse --verify -q "$REF:apps/bash/src/$NAME" >/dev/null; then
      say "source: mirror $REF"
      git cat-file blob "$REF:apps/bash/src/$NAME" > "$CACHE/$NAME.part" || die "cannot read $REF:$NAME"
      verify "$CACHE/$NAME.part" || die "$REF carries different bytes than sha256 $SHA"
      mv -f "$CACHE/$NAME.part" "$CACHE/$NAME"
      if git rev-parse --verify -q "$REF:apps/bash/src/$SIG" >/dev/null; then
        git cat-file blob "$REF:apps/bash/src/$SIG" > "$CACHE/$SIG"
      fi
      gpg_verify "$CACHE/$NAME" "$CACHE/$SIG" || true
    else
      say "ref $REF has no apps/bash/src/$NAME (fetch it: git fetch origin fantuan-apps)"
    fi
  fi
  [ -f "$CACHE/$NAME" ] || die "no source available (offline=$OFFLINE, branch=$USE_BRANCH)"
fi

verify "$CACHE/$NAME" || die "cached tarball no longer matches sha256 $SHA"

if [ "$TO_SKELETON" = "1" ]; then
  mkdir -p "$SKELETON"
  cp -f "$CACHE/$NAME" "$SKELETON/$NAME"
  [ -f "$CACHE/$SIG" ] && cp -f "$CACHE/$SIG" "$SKELETON/$SIG"
  say "skeleton: $SKELETON/$NAME"
fi

echo "$CACHE/$NAME"
