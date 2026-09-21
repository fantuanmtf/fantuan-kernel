#!/usr/bin/env bash
# P1 libc-fantuan builder (x86_64 first): static archive + the C hello.
#
#   tools/build-libc.sh              # archive + hello + kernel/hello_program.bin
#   LIBC_TARGET=... tools/build-libc.sh
#
# Outputs under build/libc-fantuan/:
#   libc-fantuan.a   deterministic archive (llvm-ar rcsD)
#   hello.elf        static ET_EXEC built from user/hello.c
# and a copy at kernel/hello_program.bin (embedded by kernel/build.rs when
# present; a fresh build without it is unchanged). The build is byte-
# reproducible for fixed compiler and inputs; --verify reruns and compares.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TARGET="${LIBC_TARGET:-x86_64-unknown-none}"
CC="${LIBC_CC:-clang}"
AR="${LIBC_AR:-llvm-ar}"
VERIFY="${LIBC_VERIFY:-0}"
case "${1:-}" in
  --verify) VERIFY=1 ;;
  "") ;;
  *) echo "usage: $0 [--verify]" >&2; exit 2 ;;
esac

command -v "$CC" >/dev/null 2>&1 || { echo "build-libc: $CC not found" >&2; exit 2; }
RES_INC="$("$CC" -print-resource-dir)/include"
OUT="$ROOT/build/libc-fantuan"
OBJ="$OUT/obj"
INC="$ROOT/libc-fantuan/include"

build() {
  rm -rf "$1"
  mkdir -p "$1/obj"
  local obj="$1/obj"
  # Compile everything to objects in filename order (deterministic inputs).
  local src
  for src in "$ROOT"/libc-fantuan/src/*.c "$ROOT"/libc-fantuan/src/*.S; do
    "$CC" "${CFLAGS[@]}" -c "$src" -o "$obj/$(basename "$src").o"
  done
  "$AR" rcsD "$1/libc-fantuan.a" "$obj"/*.o
  "$CC" "${CFLAGS[@]}" -c "$ROOT/user/hello.c" -o "$obj/hello.c.o"
  "$CC" --target="$TARGET" -nostdlib -static -no-pie \
    -Wl,--build-id=none -Wl,--gc-sections \
    -Wl,-T,"$ROOT/libc-fantuan/elf.ld" \
    "$obj/crt0.S.o" "$obj/hello.c.o" "$1/libc-fantuan.a" -o "$1/hello.elf"
  # P2 process-layer test program (fork/exec/wait/signals/mmap).
  "$CC" "${CFLAGS[@]}" -c "$ROOT/user/proc_test.c" -o "$obj/proc_test.c.o"
  "$CC" --target="$TARGET" -nostdlib -static -no-pie \
    -Wl,--build-id=none -Wl,--gc-sections \
    -Wl,-T,"$ROOT/libc-fantuan/elf.ld" \
    "$obj/crt0.S.o" "$obj/proc_test.c.o" "$1/libc-fantuan.a" -o "$1/proc_test.elf"
  # P2 userland tools exec'd by dash (ls/cat): one static ELF each. Stripped:
  # their embedded copies live in the kernel, and the minimal-ELF string
  # invariants (tools/smoke-config.sh) must not see e.g. a `cat` token from
  # the tool's source-file symbol.
  local tool
  for tool in ls cat; do
    "$CC" "${CFLAGS[@]}" -c "$ROOT/user/$tool.c" -o "$obj/$tool.c.o"
    "$CC" --target="$TARGET" -nostdlib -static -no-pie \
      -Wl,--build-id=none -Wl,--gc-sections -Wl,-s \
      -Wl,-T,"$ROOT/libc-fantuan/elf.ld" \
      "$obj/crt0.S.o" "$obj/$tool.c.o" "$1/libc-fantuan.a" -o "$1/$tool.elf"
  done
}

CFLAGS=(
  --target="$TARGET" -ffreestanding -nostdinc -isystem "$RES_INC" -I "$INC"
  -fno-builtin -fno-stack-protector -fno-pic -fno-pie -mno-red-zone
  -fno-asynchronous-unwind-tables -fno-unwind-tables -O2 -Wall -Wextra -Werror
)

echo "[libc] building libc-fantuan.a + hello.elf (target $TARGET)..."
build "$OUT"

if [ "$VERIFY" = "1" ]; then
  ALT="$ROOT/build/libc-fantuan-verify"
  build "$ALT"
  sha_a=$(sha256sum "$OUT/libc-fantuan.a" | awk '{print $1}')
  sha_b=$(sha256sum "$ALT/libc-fantuan.a" | awk '{print $1}')
  elf_a=$(sha256sum "$OUT/hello.elf" | awk '{print $1}')
  elf_b=$(sha256sum "$ALT/hello.elf" | awk '{print $1}')
  pt_a=$(sha256sum "$OUT/proc_test.elf" | awk '{print $1}')
  pt_b=$(sha256sum "$ALT/proc_test.elf" | awk '{print $1}')
  ls_a=$(sha256sum "$OUT/ls.elf" | awk '{print $1}')
  ls_b=$(sha256sum "$ALT/ls.elf" | awk '{print $1}')
  cat_a=$(sha256sum "$OUT/cat.elf" | awk '{print $1}')
  cat_b=$(sha256sum "$ALT/cat.elf" | awk '{print $1}')
  if [ "$sha_a" != "$sha_b" ] || [ "$elf_a" != "$elf_b" ] || [ "$pt_a" != "$pt_b" ] \
     || [ "$ls_a" != "$ls_b" ] || [ "$cat_a" != "$cat_b" ]; then
    echo "build-libc: NOT deterministic" >&2
    exit 1
  fi
  echo "[libc] deterministic: archive $sha_a hello $elf_a"
  rm -rf "$ALT"
fi

# Embedding copies: only touch them when bytes changed (kernel build.rs watches).
mkdir -p "$ROOT/kernel"
cmp -s "$OUT/hello.elf" "$ROOT/kernel/hello_program.bin" \
  || cp "$OUT/hello.elf" "$ROOT/kernel/hello_program.bin"
cmp -s "$OUT/proc_test.elf" "$ROOT/kernel/proc_test_program.bin" \
  || cp "$OUT/proc_test.elf" "$ROOT/kernel/proc_test_program.bin"
for tool in ls cat; do
  cmp -s "$OUT/$tool.elf" "$ROOT/kernel/${tool}_program.bin" \
    || cp "$OUT/$tool.elf" "$ROOT/kernel/${tool}_program.bin"
done

echo "[libc] build/libc-fantuan/libc-fantuan.a $(stat -c %s "$OUT/libc-fantuan.a") bytes"
echo "[libc] build/libc-fantuan/hello.elf $(stat -c %s "$OUT/hello.elf") bytes -> kernel/hello_program.bin"
echo "[libc] build/libc-fantuan/proc_test.elf $(stat -c %s "$OUT/proc_test.elf") bytes -> kernel/proc_test_program.bin"
for tool in ls cat; do
  echo "[libc] build/libc-fantuan/$tool.elf $(stat -c %s "$OUT/$tool.elf") bytes -> kernel/${tool}_program.bin"
done
