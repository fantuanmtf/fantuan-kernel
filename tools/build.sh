#!/usr/bin/env bash
# Build order: user program -> kernel (embeds it) -> bootloader.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "[user] building userland program..."
cargo build -p fantuan-user --target x86_64-unknown-none --release
cp target/x86_64-unknown-none/release/fantuan-user kernel/user_program.bin

echo "[kernel] building kernel..."
cargo build -p fantuan-kernel --target x86_64-unknown-none --release

echo "[boot] building bootloader..."
cargo build -p fantuan-boot --target x86_64-unknown-uefi --release
