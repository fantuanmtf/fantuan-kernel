#!/usr/bin/env bash
# C1 config smoke: profile invariants, the 300 MiB boot+kernel+shell budget
# and config incrementality.
#
#   phase net:     .config = net; boot log shows the net:/rump: markers and
#                  reaches the shell.
#   phase minimal: .config = minimal; boot log shows none of them and still
#                  reaches the shell.
#   budget:        built boot + kernel + shell artifacts (exact file set in
#                  ARTIFACTS below) <= 300 MiB. The check is first run with a
#                  1-byte limit to prove it rejects an over-budget set.
#   increment:     flip DEBUG_SELFTEST, rebuild x86_64 and assert the cargo
#                  "Compiling" list is a proper subset of the full crate set
#                  (fantuan-abi has no kconfig build.rs and must not rebuild).
#
# Optional: SMOKE_CONFIG_EXTRA=1 also builds the minimal profile for riscv64
# and i686. The script restores (or removes) .config on exit.
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

boot() { # log timeout
  # Build once outside the timeout: the config flip rebuilds three crates, and
  # that must not eat the boot window (run.sh's own build is then incremental).
  rm -f "$1"
  ./tools/build.sh >/dev/null 2>&1 || fail "build before boot"
  ( timeout --signal=KILL "$2" ./tools/run.sh < /dev/null > "$1" 2>&1 ) 2>/dev/null || true
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
NET_LOG="build/smoke-config-net.log"
boot "$NET_LOG" "${SMOKE_CONFIG_TIMEOUT:-90}"
if grep -q "shell: ready" "$NET_LOG" \
   && grep -q "net: lo0 up 127.0.0.1/8" "$NET_LOG" \
   && grep -q "rump: mbuf self-test ok" "$NET_LOG" \
   && ! grep -q "net: ip4 FAILED" "$NET_LOG"; then
  ok "net profile: net:/rump: markers + shell"
  grep -aE "net: lo0 up|rump: mbuf self-test ok|shell: ready" "$NET_LOG" | head -3
else
  echo "SMOKE FAIL (config: net profile) — log tail:"
  tail -25 "$NET_LOG"
  exit 1
fi

# C4 default: with no .config, kconfig.py assumes and build.sh materializes
# the `minimal` profile; the kernel must have no kernel-net cargo edge.
echo "[phase minimal] no .config -> minimal profile (C4 default)..."
rm -f .config
if ! python3 tools/kconfig.py --text 2>/dev/null | grep -q "profile minimal"; then
  fail "kconfig.py did not assume the minimal profile without .config"
fi
MIN_LOG="build/smoke-config-min.log"
boot "$MIN_LOG" "${SMOKE_CONFIG_TIMEOUT:-90}"
grep -q "^# profile: minimal" .config || fail "build.sh did not materialize the minimal profile"
if cargo tree -p fantuan-kernel --target x86_64-unknown-none -e normal --offline 2>/dev/null \
     | grep -q "kernel-net"; then
  fail "minimal profile: cargo tree still has a kernel-net edge"
fi
if grep -q "shell: ready" "$MIN_LOG" \
   && grep -q "root@Fantuan-MTF" "$MIN_LOG" \
   && ! grep -q "net: lo0 up" "$MIN_LOG" \
   && ! grep -q "rump:" "$MIN_LOG" \
   && ! grep -q "net: tcp" "$MIN_LOG"; then
  ok "minimal profile: shell + zero net:/rump: code (no kernel-net edge)"
else
  echo "SMOKE FAIL (config: minimal profile) — log tail:"
  tail -25 "$MIN_LOG"
  exit 1
fi

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
  echo "[extra] minimal builds for riscv64 and i686..."
  python3 tools/kconfig.py --profile minimal >/dev/null || fail "minimal before extras"
  cargo build -p kernel-riscv --target riscv64gc-unknown-none-elf --release \
    > build/smoke-config-riscv.log 2>&1 || fail "riscv64 minimal build"
  ./tools/build-i686.sh > build/smoke-config-i686.log 2>&1 || fail "i686 minimal build"
  ok "extra arches: riscv64 + i686 minimal builds"
fi

ok "offline gate (profiles + budget + incrementality)"
exit 0
