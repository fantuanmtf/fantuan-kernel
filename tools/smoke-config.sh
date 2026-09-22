#!/usr/bin/env bash
# C1 config smoke: profile invariants, the 300 MiB boot+kernel+shell budget
# and config incrementality. Extended in C5 with the rescue/tool string
# invariants and the minimal command-table proof.
#
#   phase net:     .config = net; boot log shows the net:/rump: markers and
#                  reaches the shell; the tools are linked, the rescue
#                  commands are not (CONFIG_RESCUE_REPAIR=n in net).
#   phase minimal: .config = minimal; boot log shows none of them, still
#                  reaches the shell, types `help` and lists only the core
#                  builtins (help, bootinfo).
#   strings:       the minimal ELF contains no net/rump/tls/rescue/tool
#                  strings; the rescue profile links the rescue commands
#                  without the tools.
#   budget:        built boot + kernel + shell artifacts (exact file set in
#                  ARTIFACTS below) <= 300 MiB. The check is first run with a
#                  1-byte limit to prove it rejects an over-budget set.
#   increment:     flip DEBUG_SELFTEST, rebuild x86_64 and assert the cargo
#                  "Compiling" list is a proper subset of the full crate set
#                  (fantuan-abi has no kconfig build.rs and must not rebuild).
#
# Optional: SMOKE_CONFIG_EXTRA=1 also builds the minimal profile for riscv64,
# i686 and aarch64. The script restores (or removes) .config on exit.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build
export PATH="$HOME/.cargo/bin:$PATH"

SAVED="build/smoke-config-saved.config"
HAVE_SAVED=0
if [ -f .config ]; then cp .config "$SAVED"; HAVE_SAVED=1; fi
cleanup() {
  if [ "$HAVE_SAVED" = "1" ]; then cp "$SAVED" .config; else rm -f .config; fi
  rm -f "$SAVED"
  rm -rf build/config
}
trap cleanup EXIT

fail() { echo "SMOKE FAIL (config): $*"; exit 1; }
ok() { echo "SMOKE PASS (config: $*)"; }

ELF="$ROOT/target/x86_64-unknown-none/release/fantuan-kernel"

# Whole-token match on the ELF's printable strings, so `cat`/`ping` cannot
# false-positive inside another word (e.g. "certificate"). grep must consume
# all input (no -q): under `pipefail` an early grep exit would SIGPIPE
# `strings` and turn a match into a non-zero pipeline.
elf_has_string() { # elf name
  strings -a "$1" | grep -E "(^|[^A-Za-z0-9_-])$2([^A-Za-z0-9_-]|$)" > /dev/null
}

# The command names that must disappear from the minimal/default ELF (their
# tables and implementations are gated by CONFIG_RESCUE_REPAIR/CONFIG_TOOLS/
# CONFIG_IMAGER). `part` cannot be a string invariant because the boot-time
# VFS already prints "part: LBA ...".
RESCUE_STRINGS=(diskhealth lsmnt lsos mount umount cat hwdiag lsdev grub-fix crypto-selftest)
TOOL_STRINGS=(ping nslookup wget)
# M12: the imager is its own symbol, enabled in the rescue/net/tls/desktop
# profiles (not minimal), so `clone` is present in net and absent in minimal.
IMAGER_STRINGS=(clone)

boot() { # log timeout
  # Build once outside the timeout: the config flip rebuilds three crates, and
  # that must not eat the boot window (run.sh's own build is then incremental).
  rm -f "$1"
  ./tools/build.sh >/dev/null 2>&1 || fail "build before boot"
  ( timeout --signal=KILL "$2" ./tools/run.sh < /dev/null > "$1" 2>&1 ) 2>/dev/null || true
}

# Minimal-shell boot: type `help` mid-run (the UART holds the bytes until the
# shell polls; the pipe stays open so the boot window is unchanged).
boot_help() { # log timeout
  rm -f "$1"
  ./tools/build.sh >/dev/null 2>&1 || fail "build before boot"
  ( ( sleep 20; printf 'help\n'; sleep 40 ) \
    | timeout --signal=KILL "$2" ./tools/run.sh > "$1" 2>&1 ) 2>/dev/null || true
}

echo "[phase net] .config = net profile..."
python3 tools/kconfig.py --profile net >/dev/null || fail "writing the net profile"
python3 tools/kconfig.py --check || fail "net profile validation"
python3 tools/kconfig.py --emit >/dev/null || fail "--emit"
grep -q "pub const CONFIG_NET: bool = true" build/config/features.rs || fail "features.rs net"
grep -q "^config_net=y" build/config/features.env || fail "features.env net"
if cargo tree -p fantuan-kernel --target x86_64-unknown-none -e normal --offline \
     --features kconfig-net 2>/dev/null | grep -q "kernel-net"; then
  ok "net profile: cargo tree has the kernel-net edge"
else
  fail "net profile: cargo tree has no kernel-net edge"
fi
if cargo tree -p kernel-aarch64 --target aarch64-unknown-none -e normal --offline \
     --features kconfig-net 2>/dev/null | grep -q "kernel-net"; then
  ok "net profile: aarch64 cargo tree has the kernel-net edge"
else
  fail "net profile: aarch64 cargo tree has no kernel-net edge"
fi
NET_LOG="build/smoke-config-net.log"
boot "$NET_LOG" "${SMOKE_CONFIG_TIMEOUT:-90}"
if ! elf_has_string "$ELF" nslookup; then
  fail "net profile: R7 tools not linked (no nslookup string)"
fi
for s in "${RESCUE_STRINGS[@]}"; do
  if elf_has_string "$ELF" "$s"; then
    fail "net profile: rescue command '$s' leaked (CONFIG_RESCUE_REPAIR=n)"
  fi
done
for s in "${IMAGER_STRINGS[@]}"; do
  elf_has_string "$ELF" "$s" || fail "net profile: imager command '$s' missing (CONFIG_IMAGER=y)"
done
if grep -q "shell: ready" "$NET_LOG" \
   && grep -q "net: lo0 up 127.0.0.1/8" "$NET_LOG" \
   && grep -q "rump: mbuf self-test ok" "$NET_LOG" \
   && ! grep -q "net: ip4 FAILED" "$NET_LOG"; then
  ok "net profile: net:/rump: markers + shell; tools linked, rescue commands absent"
  grep -aE "net: lo0 up|rump: mbuf self-test ok|shell: ready" "$NET_LOG" | head -3
else
  echo "SMOKE FAIL (config: net profile) — log tail:"
  tail -25 "$NET_LOG"
  exit 1
fi

# C5 default: with no .config, kconfig.py assumes and build.sh materializes
# the `minimal` profile (SHELL only); the kernel must have no kernel-net edge
# and no rescue/tool command strings.
echo "[phase minimal] no .config -> minimal profile (C5 default)..."
rm -f .config
if ! python3 tools/kconfig.py --text 2>/dev/null | grep -q "profile minimal"; then
  fail "kconfig.py did not assume the minimal profile without .config"
fi
MIN_LOG="build/smoke-config-min.log"
boot_help "$MIN_LOG" "${SMOKE_CONFIG_TIMEOUT:-90}"
grep -q "^# profile: minimal" .config || fail "build.sh did not materialize the minimal profile"
if cargo tree -p fantuan-kernel --target x86_64-unknown-none -e normal --offline 2>/dev/null \
     | grep -q "kernel-net"; then
  fail "minimal profile: cargo tree still has a kernel-net edge"
fi
for s in "${RESCUE_STRINGS[@]}" "${TOOL_STRINGS[@]}" "${IMAGER_STRINGS[@]}"; do
  if elf_has_string "$ELF" "$s"; then
    fail "minimal profile: command string '$s' leaked into the kernel ELF"
  fi
done
for s in rump mbedtls "net:" "tls:"; do
  if elf_has_string "$ELF" "$s"; then
    fail "minimal profile: '$s' leaked into the kernel ELF"
  fi
done
if grep -q "shell: ready" "$MIN_LOG" \
   && grep -q "root@Fantuan-MTF> " "$MIN_LOG" \
   && grep -q "shell commands (root@Fantuan-MTF" "$MIN_LOG" \
   && grep -q "^  help        this table" "$MIN_LOG" \
   && grep -q "^  bootinfo    boot handover details" "$MIN_LOG" \
   && ! grep -qE "^  (hwdiag|lsdev|lsos|lsmnt|mount|umount|cat|diskhealth|grub-fix|crypto-selftest|clone|ping|nslookup|wget) " "$MIN_LOG" \
   && ! grep -q "net: lo0 up" "$MIN_LOG" \
   && ! grep -q "rump:" "$MIN_LOG" \
   && ! grep -q "net: tcp" "$MIN_LOG"; then
  ok "minimal profile: shell + help lists only core builtins; ELF free of net/rump/tls/rescue/tool strings"
else
  echo "SMOKE FAIL (config: minimal profile) — log tail:"
  tail -30 "$MIN_LOG"
  exit 1
fi

# Rescue profile: the command-table gate in the other direction - the rescue
# commands link, the non-default tools stay out.
echo "[phase rescue] .config = rescue profile..."
python3 tools/kconfig.py --profile rescue >/dev/null || fail "writing the rescue profile"
cargo build -p fantuan-kernel --target x86_64-unknown-none --release \
  > build/smoke-config-rescue-build.log 2>&1 || fail "rescue profile x86_64 build"
for s in diskhealth grub-fix; do
  elf_has_string "$ELF" "$s" || fail "rescue profile: '$s' missing from the ELF"
done
for s in "${IMAGER_STRINGS[@]}"; do
  elf_has_string "$ELF" "$s" || fail "rescue profile: imager command '$s' missing (CONFIG_IMAGER=y)"
done
for s in "${TOOL_STRINGS[@]}"; do
  if elf_has_string "$ELF" "$s"; then
    fail "rescue profile: tool '$s' leaked (CONFIG_TOOLS=n)"
  fi
done
ok "rescue profile: rescue commands linked, tools absent"

# Budget file set: the built x86_64 kernel (release ELF + flat BIOS binary +
# ESP copy), the embedded userland program, the UEFI bootloader (release EFI +
# ESP copy) and the BIOS chain (stage1/stage2 + x86_64 image).
ARTIFACTS=(
  target/x86_64-unknown-none/release/fantuan-kernel
  build/kernel-bios.bin
  build/esp/fantuan/kernel.bin
  kernel/user_program.bin
  target/x86_64-unknown-uefi/release/fantuan-boot.efi
  build/esp/EFI/BOOT/BOOTX64.EFI
  build/stage1.bin
  build/stage2.bin
  build/bios.img
)
BUDGET_TOTAL=0
check_budget() { # limit -- files...
  local limit="$1" total=0 f
  shift
  for f in "$@"; do
    [ -f "$f" ] && total=$((total + $(stat -c %s "$f")))
  done
  BUDGET_TOTAL="$total"
  [ "$total" -le "$limit" ]
}
echo "[budget] boot + kernel + shell <= 300 MiB..."
if check_budget 1 "${ARTIFACTS[@]}"; then
  fail "budget simulation: a 1-byte limit accepted $BUDGET_TOTAL bytes"
fi
if ! check_budget $((300 * 1024 * 1024)) "${ARTIFACTS[@]}"; then
  fail "over budget: $BUDGET_TOTAL bytes > 300 MiB"
fi
ok "budget: $BUDGET_TOTAL bytes ($((BUDGET_TOTAL / 1048576)) MiB) <= 300 MiB (1-byte simulation rejected)"

echo "[increment] flip DEBUG_SELFTEST and inspect the rebuild set..."
python3 tools/kconfig.py --profile net >/dev/null || fail "net profile before baseline"
cargo build -p fantuan-kernel --target x86_64-unknown-none --release --features kconfig-net \
  > build/smoke-config-incr-base.log 2>&1 || fail "baseline x86_64 build"
python3 tools/kconfig.py --profile net --symbol DEBUG_SELFTEST=N >/dev/null || fail "flip"
cargo build -p fantuan-kernel --target x86_64-unknown-none --release --features kconfig-net \
  > build/smoke-config-incr.log 2>&1 || fail "incremental x86_64 build"
grep -a "Compiling" build/smoke-config-incr.log || true
REBUILT="$(grep -a "Compiling" build/smoke-config-incr.log \
  | sed -E 's/^[[:space:]]*Compiling ([a-zA-Z0-9_-]+) .*/\1/' | sort -u)"
[ -n "$REBUILT" ] || fail "incrementality: nothing rebuilt after the flip"
for c in $REBUILT; do
  case " fantuan-abi kernel-core kernel-net fantuan-kernel " in
    *" $c "*) ;;
    *) fail "incrementality: unexpected crate $c" ;;
  esac
done
echo "$REBUILT" | grep -qx "fantuan-abi" && fail "incrementality: fantuan-abi rebuilt (no kconfig consumer)"
echo "$REBUILT" | grep -qx "kernel-core" || fail "incrementality: kernel-core consumer not rebuilt"
COUNT="$(printf '%s\n' "$REBUILT" | grep -c .)"
[ "$COUNT" -lt 4 ] || fail "incrementality: the whole crate set rebuilt ($REBUILT)"
ok "incrementality: rebuild set {$(echo $REBUILT | tr '\n' ' ')} is a proper subset (fantuan-abi untouched)"

if [ "${SMOKE_CONFIG_EXTRA:-0}" = "1" ]; then
  echo "[extra] minimal builds for riscv64, i686 and aarch64..."
  python3 tools/kconfig.py --profile minimal >/dev/null || fail "minimal before extras"
  cargo build -p kernel-riscv --target riscv64gc-unknown-none-elf --release \
    > build/smoke-config-riscv.log 2>&1 || fail "riscv64 minimal build"
  ./tools/build-i686.sh > build/smoke-config-i686.log 2>&1 || fail "i686 minimal build"
  cargo build -p kernel-aarch64 --target aarch64-unknown-none --release \
    > build/smoke-config-aarch64.log 2>&1 || fail "aarch64 minimal build"
  ok "extra arches: riscv64 + i686 + aarch64 minimal builds"
fi

ok "offline gate (profiles + budget + incrementality)"
exit 0
