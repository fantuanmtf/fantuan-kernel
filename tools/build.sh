#!/usr/bin/env bash
# Build order (x86_64): user program -> kernel (embeds it) -> bootloader.
# --arch riscv64 builds only the RISC-V kernel (OpenSBI is the boot path).
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Kernel config (C1): materialize the default `net` profile on first build so
# the build stays incremental; C4 flips the default to minimal.
[ -f .config ] || ./tools/kconfig.py --profile net >/dev/null

ARCH="x86_64"
while [ $# -gt 0 ]; do
  case "$1" in
    --arch) ARCH="${2:-}"; shift 2 ;;
    *) shift ;;
  esac
done

if [ "$ARCH" = "riscv64" ]; then
  echo "[user] building userland program (riscv64)..."
  cargo build -p fantuan-user --target riscv64gc-unknown-none-elf --release
  cp target/riscv64gc-unknown-none-elf/release/fantuan-user kernel-riscv/user_program.bin

  echo "[riscv] building kernel-riscv..."
  cargo build -p kernel-riscv --target riscv64gc-unknown-none-elf --release
  exit 0
fi

echo "[user] building userland program..."
cargo build -p fantuan-user --target x86_64-unknown-none --release
cp target/x86_64-unknown-none/release/fantuan-user kernel/user_program.bin

echo "[kernel] building kernel..."
cargo build -p fantuan-kernel --target x86_64-unknown-none --release

echo "[boot] building bootloader..."
cargo build -p fantuan-boot --target x86_64-unknown-uefi --release
