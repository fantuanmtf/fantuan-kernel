#!/usr/bin/env bash
# Build order (x86_64): user program -> kernel (embeds it) -> bootloader.
# --arch riscv64 builds only the RISC-V kernel (OpenSBI is the boot path).
# --arch aarch64 builds only the aarch64 kernel and flattens it to the raw
# `Image` QEMU boots (the raw path passes the DTB in x0).
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Kernel config (C5): a missing .config materializes the `minimal` profile
# (SHELL only). kernel-net is an optional dependency behind the
# kconfig-net feature: pass it only when CONFIG_NET=y.
[ -f .config ] || ./tools/kconfig.py --profile minimal >/dev/null
KERNEL_FEATURES=()
if grep -q '^CONFIG_NET=y' .config; then KERNEL_FEATURES=(--features kconfig-net); fi

ARCH="x86_64"
while [ $# -gt 0 ]; do
  case "$1" in
    --arch) ARCH="${2:-}"; shift 2 ;;
    *) shift ;;
  esac
done

# Copy an embedded payload only when its bytes changed: cargo watches
# kernel/user_program.bin, and an unconditional cp bumps the mtime and forces
# a full kernel rebuild on every run even though the build is incremental.
sync_embed() { # src dst
  cmp -s "$1" "$2" || cp "$1" "$2"
}

if [ "$ARCH" = "riscv64" ]; then
  echo "[user] building userland program (riscv64)..."
  cargo build -p fantuan-user --target riscv64gc-unknown-none-elf --release
  sync_embed target/riscv64gc-unknown-none-elf/release/fantuan-user kernel-riscv/user_program.bin

  echo "[riscv] building kernel-riscv..."
  cargo build -p kernel-riscv --target riscv64gc-unknown-none-elf --release
  exit 0
fi

if [ "$ARCH" = "aarch64" ]; then
  echo "[aarch64] building kernel-aarch64..."
  cargo build -p kernel-aarch64 --target aarch64-unknown-none --release
  # QEMU only passes the DTB in x0 through its raw-Image boot protocol; the
  # ELF is the cargo artifact and this flat image is what run.sh boots.
  OBJCOPY="$(command -v llvm-objcopy || command -v aarch64-linux-gnu-objcopy || true)"
  if [ -z "$OBJCOPY" ]; then
    echo "error: llvm-objcopy (or aarch64-linux-gnu-objcopy) not found" >&2
    exit 1
  fi
  mkdir -p build
  "$OBJCOPY" -O binary target/aarch64-unknown-none/release/kernel-aarch64 build/kernel-aarch64.bin
  echo "[aarch64] build/kernel-aarch64.bin: $(stat -c %s build/kernel-aarch64.bin) bytes"
  exit 0
fi

echo "[user] building userland program..."
cargo build -p fantuan-user --target x86_64-unknown-none --release
sync_embed target/x86_64-unknown-none/release/fantuan-user kernel/user_program.bin

echo "[kernel] building kernel..."
cargo build -p fantuan-kernel --target x86_64-unknown-none --release "${KERNEL_FEATURES[@]}"

echo "[boot] building bootloader..."
cargo build -p fantuan-boot --target x86_64-unknown-uefi --release
