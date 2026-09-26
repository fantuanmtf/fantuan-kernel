#!/usr/bin/env bash
# M12-6 GPU/PCI report smoke: the read-only `gpu` probe (identity/known-ID
# names, BAR sizes, the mapped aperture, the PCIe capability, the ACPI
# thermal hook) on deterministic QEMU display models. One bounded boot per
# model on the rescue profile (CONFIG_RESCUE_REPAIR + CONFIG_GRAPHICS):
#   std    1234:1111 QEMU stdvga    BAR0 16 MiB mapped ro, no PCIe cap
#   cirrus 1013:00b8 Cirrus GD5446  BAR0 32 MiB mapped ro
#   virtio 1af4:1050 virtio-gpu     a 64-bit MMIO BAR above 4 GiB mapped ro
# The ESP autorun (--keys) re-runs `gpu` from the shell, so the command and
# the boot block are asserted from the same transcript.
#
# The real-hardware half of M12_TOOLS_HW §7 (one AMD RX 500/6000 for the
# PCIe link + thermal report) is a manual OPERATIONS follow-up: this host
# has no AMD GPU, and QEMU's display models expose no PCIe capability, so
# the `pcie link` decode stays untested at runtime here (the fetch itself is
# exercised only structurally). The `_TZ_` thermal scan is likewise only
# proven absent (QEMU's DSDT has no thermal zone).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p build

fail() { echo "SMOKE FAIL (gpu): $*"; exit 1; }

# CONFIG_GRAPHICS is enabled by the rescue profile (M12-6).
python3 tools/kconfig.py --profile rescue >/dev/null || fail "config"
./tools/build.sh >/dev/null 2>&1 || fail "pre-build"

OK=1
note() { echo "missing: $1"; OK=0; }

assert_common() { # log
  local log="$1"
  grep -q "root@Fantuan-MTF> gpu" "$log" || note "shell gpu command"
  grep -q "gpu: pci display devices found: 1" "$log" || note "device count"
  grep -qF "gpu: pcie n/a (no PCIe capability)" "$log" || note "pcie n/a line"
  grep -qF "gpu: thermal unavailable (no ACPI TZ)" "$log" || note "thermal line"
  grep -qaE "acpi: .*dsdt=true tz=false" "$log" || note "acpi dsdt/tz summary"
  grep -qE "panic|PANIC" "$log" && { echo "panic in the log"; OK=0; }
}

boot() { # name vga
  local name="$1" vga="$2"
  local log="build/smoke-gpu-$name.log"
  rm -f "$log"
  timeout --signal=KILL "${SMOKE_GPU_TIMEOUT:-90}" \
    ./tools/run.sh --no-build --keys --vga "$vga" > "$log" < /dev/null 2>&1 || true
  echo "$log"
}

# --- -vga std: QEMU/OVMF default; the baseline identity + BAR ----------------
STD_LOG=$(boot std std)
IDENT="gpu: 00:02.0 1234:1111 QEMU stdvga [display]"
grep -qF "$IDENT" "$STD_LOG" || note "std identity line"
[ "$(grep -acF "$IDENT" "$STD_LOG")" -ge 2 ] || note "std identity via boot + shell"
grep -qE "gpu: bar0 0x[0-9a-f]+ size 16M \(mapped ro\)" "$STD_LOG" || note "std bar0 16M mapped"
grep -q "ss=1af4:1100" "$STD_LOG" || note "std subsystem ids"
assert_common "$STD_LOG"

# --- -vga cirrus: the second fixture identity and its 32 MiB aperture --------
CIR_LOG=$(boot cirrus cirrus)
IDENT="gpu: 00:02.0 1013:00b8 Cirrus GD5446 [display]"
grep -qF "$IDENT" "$CIR_LOG" || note "cirrus identity line"
[ "$(grep -acF "$IDENT" "$CIR_LOG")" -ge 2 ] || note "cirrus identity via boot + shell"
grep -qE "gpu: bar0 0x[0-9a-f]+ size 32M \(mapped ro\)" "$CIR_LOG" || note "cirrus bar0 32M mapped"
assert_common "$CIR_LOG"

# --- -vga virtio: a 64-bit BAR above 4 GiB must map through PHYS_OFFSET ------
VIRT_LOG=$(boot virtio virtio)
grep -qF "gpu: 00:02.0 1af4:1050 virtio-gpu [display]" "$VIRT_LOG" || note "virtio identity line"
grep -qE "gpu: bar[0-9] 0x[0-9a-f]+ size 16K \(mapped ro, 64-bit\)" "$VIRT_LOG" || note "virtio 64-bit bar mapped"
assert_common "$VIRT_LOG"

if [ "$OK" = "1" ]; then
  echo "SMOKE PASS (gpu: std + cirrus identity/BAR, 64-bit aperture mapped, pcie n/a, no ACPI TZ, shell command)"
  grep -aE "gpu: (00:02.0|bar[0-9]|pcie|thermal|pci display)" "$STD_LOG" | head -8
  exit 0
fi
echo "SMOKE FAIL (gpu) — std tail:"
tail -20 build/smoke-gpu-std.log
exit 1
