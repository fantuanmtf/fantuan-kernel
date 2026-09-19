#!/usr/bin/env bash
# Build the shared user crate for the i686 custom target (M10-4b3b). Same
# nightly + build-std story as kernel-i686 (targets/i686-fantuan-none.json);
# the ELF32 image lands in build/user-i686.elf for the kernel's build.rs.
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build

cargo +nightly build -p fantuan-user --release \
  -Z build-std=core -Z json-target-spec \
  --target "$ROOT/targets/i686-fantuan-none.json"
cp target/i686-fantuan-none/release/fantuan-user build/user-i686.elf
echo "build/user-i686.elf: $(stat -c %s build/user-i686.elf) bytes"
