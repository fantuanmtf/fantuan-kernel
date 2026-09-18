#!/usr/bin/env bash
# Interactive BIOS boot (M10): build the image and run it with the serial
# console on stdio, so the shell reads what you type. Ctrl-A X quits.
# Usage: tools/run-bios.sh [--arch i686]
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ARCH="x86_64"
if [ "${1:-}" = "--arch" ] && [ "${2:-}" = "i686" ]; then
  ARCH="i686"
fi

if [ "$ARCH" = "i686" ]; then
  ./tools/build-bios.sh --arch i686
  IMG=build/bios-i686.img
else
  ./tools/build-bios.sh
  IMG=build/bios.img
fi

echo "booting $IMG (Ctrl-A X quits)"
exec qemu-system-x86_64 -machine pc -m 512M \
  -drive format=raw,file="$IMG" -nographic -no-reboot
