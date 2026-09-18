#!/usr/bin/env bash
# Build the M10 BIOS boot spike: stage1 (MBR, 512 B) + stage2 (real-mode, 4 KiB).
# The result is a raw disk image bootable by SeaBIOS.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build
nasm -f bin -o build/stage1.bin boot-bios/stage1.asm
nasm -f bin -o build/stage2.bin boot-bios/stage2.asm
nasm -f bin -o build/longmode.bin boot-bios/longmode.asm
cat build/stage1.bin build/stage2.bin build/longmode.bin > build/bios.img
s1=$(stat -c %s build/stage1.bin)
s2=$(stat -c %s build/stage2.bin)
s3=$(stat -c %s build/longmode.bin)
[ "$s1" -eq 512 ] || { echo "stage1 is $s1 bytes (must be 512)"; exit 1; }
[ "$s2" -eq 4096 ] || { echo "stage2 is $s2 bytes (must be 4096)"; exit 1; }
[ "$s3" -le 4096 ] || { echo "longmode payload is $s3 bytes (max 4096)"; exit 1; }
# Pad the image so the loader's sector requests always stay inside the disk.
truncate -s 1048576 build/bios.img
echo "build/bios.img: $(stat -c %s build/bios.img) bytes (stage1 512 + stage2 4096 + payload $s3)"
