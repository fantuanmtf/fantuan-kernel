#!/usr/bin/env bash
# Build the 32-bit kernel (M10). This is the project's only nightly step:
# stable has no bare-metal i686 target, so the crate pins nightly + rust-src
# and builds core from source for targets/i686-fantuan-none.json.
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Kernel config (C1): materialize the default `net` profile on first build.
[ -f "$ROOT/.config" ] || "$ROOT/tools/kconfig.py" --profile net >/dev/null
cd "$ROOT/kernel-i686"

# Cargo treats --target <path.json> as an opaque string: editing the spec
# does not invalidate cached rlibs, which once produced a mixed-ABI binary.
# Force a clean build whenever the spec content changes.
SPEC="$ROOT/targets/i686-fantuan-none.json"
STAMP="$ROOT/kernel-i686/.target-spec.sha"
HASH="$(sha256sum "$SPEC" | cut -d' ' -f1)"
if [ -f "$STAMP" ] && [ "$(cat "$STAMP")" != "$HASH" ]; then
  echo "[i686] target spec changed - cleaning the crate cache"
  cargo clean
fi
echo "$HASH" > "$STAMP"

# The kernel embeds the shared user crate as an ELF32 image (M10-4b3b).
"$ROOT/tools/build-user-i686.sh" >/dev/null

cargo build --release
objcopy -O binary target/i686-fantuan-none/release/kernel-i686 "$ROOT/build/kernel-i686.bin"
echo "build/kernel-i686.bin: $(stat -c %s "$ROOT/build/kernel-i686.bin") bytes"
