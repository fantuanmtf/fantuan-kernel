#!/usr/bin/env bash
# P3 bash builder (docs/POSIX_PLAN.md): cross-configure and build the pristine
# GNU Bash 5.3 (GPLv3, apps/bash/) for the fantuan native ABI, link it against
# libc-fantuan, strip it and embed it as kernel/bash_program.bin (plus the
# kernel/bash_program.sha256 marker the licensing gates read). The sources are
# obtained by tools/fetch-bash-src.sh - they are not tracked on main. bash
# stays an app-layer program: this script never links it into the kernel or the
# base libraries (the GPL firewall), and the default tools/build.sh runs it.
#
#   tools/build-libc.sh          # first: the archive this links against
#   tools/build-bash.sh          # build build/bash/bash.elf
#   BASH_VERIFY=1 tools/build-bash.sh   # rebuild and compare hashes
#   BASH_KEEP_SRC=1 ...          # keep/reuse an existing extracted tree
#
# Configuration: --host=x86_64-unknown-none --disable-nls --disable-readline
# --without-bash-malloc --enable-static-link, with the freestanding clang and
# -nostdlib so configure's link probes resolve against libc-fantuan and the
# host glibc can never leak into the result. Outputs: build/bash/{bash.elf,
# configure.log,make.log,patch.log}; on success kernel/bash_program.bin
# (embedded by kernel/build.rs when present).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# The pristine tarball is not tracked on main (licensing: THIRD_PARTY.md);
# tools/fetch-bash-src.sh resolves it from the cache, a vendored tree, the
# pinned upstream URL or the fantuan-apps branch, and verifies its sha256.
TARBALL="${BASH_TARBALL:-$ROOT/apps/bash/src/bash-5.3.tar.gz}"
if [ ! -f "$TARBALL" ]; then
  TARBALL="$("$ROOT/tools/fetch-bash-src.sh")" || {
    echo "build-bash: cannot obtain bash-5.3.tar.gz (see the guidance above)" >&2
    exit 2
  }
fi
OUT="$ROOT/build/bash"
SRC="$OUT/src/bash-5.3"
CC="${BASH_CC:-clang}"
AR="${BASH_AR:-llvm-ar}"
RANLIB="${BASH_RANLIB:-llvm-ranlib}"
TARGET="${BASH_TARGET:-x86_64-unknown-none}"
VERIFY="${BASH_VERIFY:-0}"
JOBS="${BASH_JOBS:-$(nproc 2>/dev/null || echo 2)}"
LIBC_INC="$ROOT/libc-fantuan/include"
LIBC_A="$ROOT/build/libc-fantuan/libc-fantuan.a"
CRT0="$ROOT/build/libc-fantuan/obj/crt0.S.o"
export LC_ALL=C

command -v "$CC" >/dev/null 2>&1 || { echo "build-bash: $CC not found" >&2; exit 2; }
command -v bison >/dev/null 2>&1 || { echo "build-bash: bison not found (parse.y)" >&2; exit 2; }
[ -f "$TARBALL" ] || { echo "build-bash: $TARBALL missing (run tools/fetch-bash-src.sh)" >&2; exit 2; }
[ -f "$LIBC_A" ] && [ -f "$CRT0" ] || {
  echo "build-bash: libc-fantuan archive missing; run tools/build-libc.sh first" >&2
  exit 2
}

# Install the built program as the embedded payload plus the sha256 marker the
# licensing gates read (kernel/build.rs bakes the hash in as SHELL_IMAGE_SHA256).
install_artifact() {
  local size sha
  size="$(stat -c %s "$OUT/bash.elf")"
  sha="$(sha256sum "$OUT/bash.elf" | awk '{print $1}')"
  mkdir -p "$ROOT/kernel"
  cmp -s "$OUT/bash.elf" "$ROOT/kernel/bash_program.bin" \
    || cp "$OUT/bash.elf" "$ROOT/kernel/bash_program.bin"
  printf '%s\n' "$sha" > "$ROOT/kernel/bash_program.sha256"
  echo "[bash] build/bash/bash.elf $size bytes sha256=$sha -> kernel/bash_program.bin"
  echo "[bash] marker: kernel/bash_program.sha256"
}

# Incremental skip: configure+make is the expensive part of the default build,
# so stamp the inputs (tarball, patches, libc archive, this script) and reuse
# the existing binary while they are unchanged. BASH_FORCE=1 rebuilds anyway;
# BASH_VERIFY=1 does too (it has to, to compare two builds).
stamp_value() {
  { sha256sum "$TARBALL" "$LIBC_A" "$0" 2>/dev/null | cut -d' ' -f1
    cat "$ROOT"/apps/bash/patches/*.patch 2>/dev/null | sha256sum | cut -d' ' -f1
  } | sha256sum | cut -d' ' -f1
}
STAMP="$OUT/.stamp"
WANT_STAMP="$(stamp_value)"
if [ "${BASH_FORCE:-0}" != "1" ] && [ "$VERIFY" != "1" ] \
   && [ -f "$OUT/bash.elf" ] && [ "$(cat "$STAMP" 2>/dev/null || true)" = "$WANT_STAMP" ]; then
  echo "[bash] up to date (tarball, patches and libc unchanged); reusing build/bash/bash.elf"
  install_artifact
  exit 0
fi

if [ "${BASH_KEEP_SRC:-0}" != "1" ] || [ ! -f "$SRC/configure" ]; then
  rm -rf "$OUT/src"
  mkdir -p "$OUT/src"
  tar xzf "$TARBALL" -C "$OUT/src" || { echo "build-bash: extract failed" >&2; exit 2; }
  # Replay the registered portability patches (apps/bash/manifest.toml). The
  # manifest is the list of record: every patch it names must actually be
  # applied, so a missing file cannot silently build an unpatched bash.
  : > "$OUT/patch.log"
  APPLIED=0
  for p in "$ROOT"/apps/bash/patches/*.patch; do
    [ -e "$p" ] || continue
    echo "[bash] patch $(basename "$p")"
    patch -p1 -d "$SRC" --batch --forward < "$p" >> "$OUT/patch.log" 2>&1 || {
      echo "build-bash: patch $(basename "$p") failed (see $OUT/patch.log)" >&2
      exit 1
    }
    APPLIED=$((APPLIED + 1))
  done
  WANT="$(grep -o '"patches/[^"]*"' "$ROOT/apps/bash/manifest.toml" | wc -l)"
  [ "$APPLIED" = "$WANT" ] || {
    echo "build-bash: applied $APPLIED patch(es), manifest lists $WANT" >&2
    exit 1
  }
else
  ( cd "$SRC" && make distclean >/dev/null 2>&1 || true )
fi
[ -f "$SRC/configure" ] || { echo "build-bash: unexpected tarball layout" >&2; exit 2; }

RES_INC="$("$CC" -print-resource-dir)/include"
CFLAGS=(
  "--target=$TARGET" -ffreestanding -nostdinc -isystem "$RES_INC" -I "$LIBC_INC"
  -std=gnu11 -fno-builtin -fno-stack-protector -fno-pic -fno-pie
  -mno-red-zone -fno-asynchronous-unwind-tables -fno-unwind-tables
  -fno-common -O2 -Wall
)
LDFLAGS_PROBE="-nostdlib -static -no-pie -Wl,--build-id=none"
LINK_LDFLAGS="-nostdlib -static -no-pie -Wl,--build-id=none -Wl,-s -Wl,--gc-sections"
PROBE_LIBS="$CRT0 $LIBC_A"

echo "[bash] configuring ($TARGET, no glibc: probes link against libc-fantuan)..."
(
  cd "$SRC" && \
  env CC="$CC" AR="$AR" RANLIB="$RANLIB" CFLAGS="${CFLAGS[*]}" \
      CFLAGS_FOR_BUILD="-g -DCROSS_COMPILING -std=gnu11" \
      LDFLAGS="$LDFLAGS_PROBE" LIBS="$PROBE_LIBS" YACC=bison \
      bash_cv_func_strchrnul_works=yes \
      bash_cv_getcwd_malloc=yes \
      ./configure \
        --host="$TARGET" \
        --build="$(uname -m)-pc-linux-gnu" \
        --without-bash-malloc \
        --disable-nls \
        --disable-readline \
        --enable-static-link
) > "$OUT/configure.log" 2>&1 || {
  echo "build-bash: configure failed (see $OUT/configure.log)" >&2
  tail -20 "$OUT/configure.log" >&2
  exit 1
}

echo "[bash] building (make -j$JOBS)..."
(
  cd "$SRC" && make -j"$JOBS" ${BASH_MAKE_EXTRA:-} \
      LDFLAGS="$LINK_LDFLAGS" \
      CFLAGS_FOR_BUILD="-g -DCROSS_COMPILING -std=gnu11" \
      LIBS_FOR_BUILD= \
      YACC=bison
) > "$OUT/make.log" 2>&1 || {
  echo "build-bash: build failed (see $OUT/make.log)" >&2
  grep -nE "error:|Error [0-9]" "$OUT/make.log" | head -20 >&2
  exit 1
}

[ -f "$SRC/bash" ] || { echo "build-bash: no bash binary produced" >&2; exit 1; }
cp "$SRC/bash" "$OUT/bash.elf"
printf '%s\n' "$WANT_STAMP" > "$STAMP"

if [ "$VERIFY" = "1" ]; then
  KEEP_OLD="$OUT/bash.elf.keep"
  mv "$OUT/bash.elf" "$KEEP_OLD"
  BASH_VERIFY=0 BASH_KEEP_SRC=0 BASH_FORCE=1 "$0" >/dev/null 2>&1 || {
    echo "build-bash: verify rebuild failed" >&2; exit 1; }
  SHA2="$(sha256sum "$OUT/bash.elf" | awk '{print $1}')"
  SHA1="$(sha256sum "$KEEP_OLD" | awk '{print $1}')"
  rm -f "$KEEP_OLD"
  if [ "$SHA1" != "$SHA2" ]; then
    echo "build-bash: NOT deterministic ($SHA1 != $SHA2)" >&2
    exit 1
  fi
  echo "[bash] deterministic: $SHA1"
fi

install_artifact
