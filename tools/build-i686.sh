#!/usr/bin/env bash
# Build the 32-bit kernel (M10). This is the project's only nightly step:
# stable has no bare-metal i686 target, so the crate pins nightly + rust-src
# and builds core from source for targets/i686-fantuan-none.json.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/kernel-i686"
cargo build --release
objcopy -O binary target/i686-fantuan-none/release/kernel-i686 "$ROOT/build/kernel-i686.bin"
echo "build/kernel-i686.bin: $(stat -c %s "$ROOT/build/kernel-i686.bin") bytes"
