#!/usr/bin/env bash
# C5 bash early-port spike (M14-4/M14-8 groundwork).
#
# Attempts the real GNU Bash 5.3 cross-build against the project's bare-metal
# x86_64 target and records a deterministic blocker list: the configure
# failure, the first N missing libc/POSIX headers and the first N missing
# libc/POSIX symbols. The build is EXPECTED to fail until the M14-4 POSIX
# layer lands; the script exits 0 when the report was produced.
#
#   tools/build-bash-spike.sh                  # record blockers (exit 0)
#   BASH_SPIKE_STRICT=1 tools/build-bash-spike.sh   # exit 1 while blocked
#   BASH_SPIKE_TARGET=riscv64-unknown-none-elf ...  # another bare target
#   BASH_SPIKE_HEADERS=10 BASH_SPIKE_SYMBOLS=15 ... # report caps
#   BASH_SPIKE_LIBC_INC=libc-fantuan/include \
#   BASH_SPIKE_LIBC_A=build/libc-fantuan/libc-fantuan.a \
#       tools/build-bash-spike.sh   # measure what libc-fantuan now covers
#
# Outputs (all under build/bash-spike/):
#   configure.log        raw configure transcript
#   missing-headers.txt  probed headers the target libc does not provide
#   missing-symbols.txt  probed POSIX symbols left undefined by the compiler
#   provided-symbols.txt (with BASH_SPIKE_LIBC_A) symbols the archive defines
#   blockers.txt         the report (also printed)
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

WORK="$ROOT/build/bash-spike"
TARBALL="${BASH_TARBALL:-$ROOT/apps/bash/src/bash-5.3.tar.gz}"
[ -f "$TARBALL" ] || TARBALL="$("$ROOT/tools/fetch-bash-src.sh" 2>/dev/null)" || TARBALL=""
TARGET="${BASH_SPIKE_TARGET:-x86_64-unknown-none}"
MAX_HEADERS="${BASH_SPIKE_HEADERS:-12}"
MAX_SYMBOLS="${BASH_SPIKE_SYMBOLS:-20}"
CFG_TIMEOUT="${BASH_SPIKE_CONFIGURE_TIMEOUT:-120}"

command -v clang >/dev/null 2>&1 || { echo "bash-spike: clang not found" >&2; exit 2; }
[ -f "$TARBALL" ] || { echo "bash-spike: no bash-5.3.tar.gz (run tools/fetch-bash-src.sh)" >&2; exit 2; }

rm -rf "$WORK"
mkdir -p "$WORK"
tar -xzf "$TARBALL" -C "$WORK" || { echo "bash-spike: extract failed" >&2; exit 2; }
SRC="$WORK/bash-5.3"

RES_INC="$(clang -print-resource-dir)/include"
CFLAGS=(--target="$TARGET" -ffreestanding -nostdinc -isystem "$RES_INC")
LIBC_INC="${BASH_SPIKE_LIBC_INC:-}"
LIBC_A="${BASH_SPIKE_LIBC_A:-}"
# Absolutise: configure runs from the extracted source directory, where a
# repo-relative -I path would no longer resolve (and every probe would fail).
if [ -n "$LIBC_INC" ] && [ -d "$LIBC_INC" ]; then
  LIBC_INC="$(cd "$LIBC_INC" && pwd)"
fi
if [ -n "$LIBC_A" ] && [ -f "$LIBC_A" ]; then
  LIBC_A="$(cd "$(dirname "$LIBC_A")" && pwd)/$(basename "$LIBC_A")"
fi
if [ -n "$LIBC_INC" ]; then
  CFLAGS+=(-I "$LIBC_INC")
fi
CC_STR="clang ${CFLAGS[*]}"

echo "[1/3] configure attempt (host $TARGET)..."
( cd "$SRC" && timeout "$CFG_TIMEOUT" env CC="$CC_STR" AR=llvm-ar RANLIB=llvm-ranlib \
    ./configure \
      --host="$TARGET" \
      --build="$(uname -m)-pc-linux-gnu" \
      --without-bash-malloc \
      --disable-nls \
      --disable-readline \
      --enable-static-link \
      > "$WORK/configure.log" 2>&1 )
CFG_EXIT=$?
CONFIGURE_BLOCKER="$(grep -m1 '^configure: error:' "$WORK/configure.log" || true)"
[ -n "$CONFIGURE_BLOCKER" ] || CONFIGURE_BLOCKER="configure exited $CFG_EXIT without a 'configure: error:' line"
echo "  $CONFIGURE_BLOCKER"

echo "[2/3] header probes (first $MAX_HEADERS recorded)..."
HEADERS=(
  stdio.h stdlib.h string.h strings.h unistd.h fcntl.h errno.h signal.h
  setjmp.h stdarg.h stddef.h stdint.h limits.h inttypes.h math.h
  sys/types.h sys/stat.h sys/wait.h sys/times.h sys/resource.h sys/time.h
  sys/select.h sys/socket.h sys/ioctl.h sys/file.h sys/param.h
  termios.h pwd.h grp.h dirent.h time.h wchar.h wctype.h
  glob.h locale.h langinfo.h iconv.h wordexp.h poll.h dlfcn.h
  paths.h libintl.h regex.h
)
: > "$WORK/header-probes.log"
: > "$WORK/missing-headers.txt"
for h in "${HEADERS[@]}"; do
  if ! echo "#include <$h>" | clang "${CFLAGS[@]}" -fsyntax-only -x c - \
       >> "$WORK/header-probes.log" 2>&1; then
    echo "$h" >> "$WORK/missing-headers.txt"
  fi
done
MISSING_HEADER_TOTAL="$(wc -l < "$WORK/missing-headers.txt" | tr -d ' ')"

echo "[3/3] POSIX symbol probes..."
SYMBOLS=(
  fork execve waitpid pipe dup dup2 _exit
  open close read write lseek fstat stat lstat access readlink symlink
  opendir readdir closedir chdir getcwd mkdir rmdir unlink rename
  fcntl ioctl isatty ttyname tcgetattr tcsetattr tcgetpgrp tcsetpgrp
  signal sigaction sigprocmask sigsuspend sigemptyset sigaddset kill killpg
  getpid getppid getuid geteuid getgid getegid setpgid umask
  forkpty openpty
  getpwnam getpwuid getgrnam getgrgid endpwent
  glob globfree fnmatch wordexp wordfree regcomp regexec regfree
  setlocale localeconv nl_langinfo mbrtowc wcrtomb mbsrtowcs wcsrtombs
  iconv_open iconv iconv_close
  time gettimeofday strftime strptime tzset localtime mktime
  getenv putenv setenv unsetenv system popen pclose
  poll select nanosleep getrusage times sysconf pathconf
  getrlimit setrlimit dlopen dlsym dlclose
)
{
  echo "/* Generated by tools/build-bash-spike.sh - do not edit. */"
  echo "/* Address-of each POSIX symbol forces a relocation; nm -u lists them. */"
  for s in "${SYMBOLS[@]}"; do echo "extern void $s(void);"; done
  echo "void *const bash_spike_symbols[] = {"
  for s in "${SYMBOLS[@]}"; do echo "    (void *)$s,"; done
  echo "};"
} > "$WORK/posix-probe.c"
clang "${CFLAGS[@]}" -fno-builtin -fno-stack-protector -c "$WORK/posix-probe.c" \
  -o "$WORK/posix-probe.o" 2> "$WORK/posix-probe.err" \
  || { echo "bash-spike: probe compile failed:" >&2; cat "$WORK/posix-probe.err" >&2; exit 2; }
NM="nm"
command -v llvm-nm >/dev/null 2>&1 && NM="llvm-nm"
"$NM" -u "$WORK/posix-probe.o" | awk '{print $NF}' | sort -u > "$WORK/undefined.txt"
: > "$WORK/missing-symbols.txt"
for s in "${SYMBOLS[@]}"; do
  if grep -qx "$s" "$WORK/undefined.txt"; then echo "$s" >> "$WORK/missing-symbols.txt"; fi
done
MISSING_SYMBOL_TOTAL="$(wc -l < "$WORK/missing-symbols.txt" | tr -d ' ')"

# Optional: measure how many probed symbols libc-fantuan.a now defines (P1).
# With an archive, "still missing" means probed-but-not-defined (the object
# file's undefined list is every probed symbol by construction).
PROVIDED_TOTAL=0
REMAINING_SYMBOL_TOTAL="$MISSING_SYMBOL_TOTAL"
: > "$WORK/provided-symbols.txt"
: > "$WORK/missing-from-libc.txt"
if [ -n "$LIBC_A" ]; then
  "$NM" --defined-only "$LIBC_A" | awk '{print $NF}' | sort -u > "$WORK/libc-defined.txt"
  for s in "${SYMBOLS[@]}"; do
    if grep -qx "$s" "$WORK/libc-defined.txt"; then
      echo "$s" >> "$WORK/provided-symbols.txt"
    else
      echo "$s" >> "$WORK/missing-from-libc.txt"
    fi
  done
  PROVIDED_TOTAL="$(wc -l < "$WORK/provided-symbols.txt" | tr -d ' ')"
  REMAINING_SYMBOL_TOTAL="$(wc -l < "$WORK/missing-from-libc.txt" | tr -d ' ')"
fi

{
  echo "# bash 5.3 early-port blocker report (C5)"
  echo "# target: $TARGET"
  echo "# configure flags: --host=$TARGET --build=$(uname -m)-pc-linux-gnu --without-bash-malloc --disable-nls --disable-readline --enable-static-link"
  echo "# compiler: $CC_STR"
  echo "# configure: $CONFIGURE_BLOCKER (exit $CFG_EXIT)"
  echo "# missing headers: $MISSING_HEADER_TOTAL of ${#HEADERS[@]} probed; first $MAX_HEADERS:"
  head -n "$MAX_HEADERS" "$WORK/missing-headers.txt" | sed 's/^/  - /'
  if [ -n "$LIBC_A" ]; then
    echo "# symbols still missing from libc-fantuan: $REMAINING_SYMBOL_TOTAL of ${#SYMBOLS[@]} probed; first $MAX_SYMBOLS:"
    head -n "$MAX_SYMBOLS" "$WORK/missing-from-libc.txt" | sed 's/^/  - /'
    echo "# libc-fantuan ($LIBC_A) provides $PROVIDED_TOTAL of ${#SYMBOLS[@]} probed symbols"
    echo "# libc-fantuan headers in the probe: $LIBC_INC"
    echo "# headers still missing: $((MISSING_HEADER_TOTAL)) of ${#HEADERS[@]}"
  else
    echo "# missing POSIX symbols: $MISSING_SYMBOL_TOTAL of ${#SYMBOLS[@]} probed; first $MAX_SYMBOLS:"
    head -n "$MAX_SYMBOLS" "$WORK/missing-symbols.txt" | sed 's/^/  - /'
  fi
} > "$WORK/blockers.txt"

cat "$WORK/blockers.txt"
if [ "$MISSING_HEADER_TOTAL" -eq 0 ] && [ "$REMAINING_SYMBOL_TOTAL" -eq 0 ]; then
  echo "bash-spike: READY (all probed headers/symbols resolve against the target)"
  exit 0
fi
echo "bash-spike: BLOCKED (expected while the POSIX/libc layer is incomplete; see apps/bash/port/)"
if [ "${BASH_SPIKE_STRICT:-0}" = "1" ] && [ "$REMAINING_SYMBOL_TOTAL" -gt 0 ]; then
  exit 1
fi
exit 0
