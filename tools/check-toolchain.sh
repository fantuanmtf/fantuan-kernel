#!/usr/bin/env bash
# Task 1 Deliverable D-1c: Idempotent toolchain + host-dependency checker (R3 mitigation).
# Verifies Rust targets, build tooling, and run-time emulation dependencies.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

FAIL=0
pass() { echo "  [ok] $1"; }
warn() { echo "  [warn] $1"; }
fail() { echo "  [FAIL] $1"; FAIL=1; }

echo "== fantuan-kernel toolchain check =="

echo "[1] Rust toolchain + required targets"
if command -v rustup >/dev/null 2>&1; then
  RUSTC_V=$(rustc --version 2>/dev/null || echo "(rustc missing)")
  echo "  rustup present; $RUSTC_V"
  for T in x86_64-unknown-uefi x86_64-unknown-none; do
    if rustup target list --installed 2>/dev/null | grep -q "^$T$"; then
      pass "target $T installed"
    else
      warn "target $T missing — attempting install"
      if rustup target add "$T" 2>/dev/null; then
        pass "target $T installed via rustup"
      else
        fail "target $T could not be installed"
      fi
    fi
  done
else
  fail "rustup not found; install via https://rustup.rs"
fi

echo "[2] Build-time C / binary tools"
for CMD in cc objcopy ld; do
  if command -v "$CMD" >/dev/null 2>&1; then pass "$CMD ($(which "$CMD"))"; else fail "$CMD missing"; fi
done

echo "[3] Run-time emulation + test helpers"
if command -v qemu-system-x86_64 >/dev/null 2>&1; then
  QV=$(qemu-system-x86_64 --version 2>/dev/null | head -n 1)
  pass "qemu-system-x86_64: $QV"
else
  fail "qemu-system-x86_64 missing — install package qemu-system-x86"
fi
for CMD in python3; do
  if command -v "$CMD" >/dev/null 2>&1; then pass "$CMD ($($CMD --version 2>&1 | head -n 1))"; else fail "$CMD missing"; fi
done

echo "[4] OVMF firmware availability (for smoke/run helpers)"
FOUND_OVMF=0
for d in /usr/share/edk2/x64 /usr/share/OVMF /usr/share/ovmf; do
  if [ -f "$d/OVMF_VARS.4m.fd" ] && [ -f "$d/OVMF_CODE.4m.fd" ]; then
    pass "OVMF 4MiB builds found at $d"
    FOUND_OVMF=1
    break
  fi
done
if [ "$FOUND_OVMF" -eq 0 ]; then
  for d in /usr/share/edk2/x64 /usr/share/OVMF /usr/share/ovmf; do
    if [ -f "$d/OVMF_VARS.fd" ] && [ -f "$d/OVMF_CODE.fd" ]; then
      pass "OVMF legacy builds found at $d"
      FOUND_OVMF=1
      break
    fi
  done
fi
if [ "$FOUND_OVMF" -eq 0 ]; then
  warn "OVMF firmware not in standard paths — smoke.sh may need OVMF in build/ovmf-smm/ or build/"
fi
if [ -f build/ovmf-smm/OVMF_CODE_4M.ms.fd ] && [ -f build/ovmf-smm/OVMF_VARS_4M.fd ]; then
  pass "SMM OVMF build present at build/ovmf-smm/ (NVRAM repair smoke will run)"
else
  warn "SMM OVMF build not present at build/ovmf-smm/ — NVRAM repair phase in smoke.sh will be SKIPPED (non-fatal for M0–M7.6)"
fi

echo "== summary =="
if [ "$FAIL" -eq 0 ]; then
  echo "PASS: all required toolchain components present"
  exit 0
else
  echo "FAIL: missing required components — see [FAIL] lines above"
  exit 1
fi
