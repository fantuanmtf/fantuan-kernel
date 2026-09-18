#!/usr/bin/env bash
# Build the M10 BIOS boot image: stage1 (MBR) + stage2 (real mode + pm32) +
# the x86_64 kernel flat binary loaded by stage2 with ATA PIO.
set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build

STAGE2_SECTORS=32
KERNEL_LBA=$((1 + STAGE2_SECTORS))

echo "[1/3] building the x86_64 kernel..."
cargo build -p fantuan-kernel --target x86_64-unknown-none --release >/dev/null
objcopy -O binary target/x86_64-unknown-none/release/fantuan-kernel build/kernel-bios.bin
KSIZE=$(stat -c %s build/kernel-bios.bin)
KSEC=$(( (KSIZE + 511) / 512 ))

echo "[2/3] assembling stage1 + stage2 (kernel: $KSIZE bytes, $KSEC sectors)..."
nasm -f bin -o build/stage1.bin boot-bios/stage1.asm
nasm -f bin -I boot-bios/ -D KERNEL_LBA=$KERNEL_LBA -D KERNEL_SECTORS=$KSEC \
  -o build/stage2.bin boot-bios/stage2.asm
s1=$(stat -c %s build/stage1.bin)
s2=$(stat -c %s build/stage2.bin)
[ "$s1" -eq 512 ] || { echo "stage1 is $s1 bytes (must be 512)"; exit 1; }
[ "$s2" -eq $((STAGE2_SECTORS * 512)) ] || { echo "stage2 is $s2 bytes (must be $((STAGE2_SECTORS * 512)))"; exit 1; }

echo "[3/3] linking build/bios.img..."
cat build/stage1.bin build/stage2.bin build/kernel-bios.bin > build/bios.img
# Pad so no sector request can run past the end of the disk.
minsz=$(( (KERNEL_LBA * 512) + KSIZE + 1048576 ))
cur=$(stat -c %s build/bios.img)
want=$(( (minsz + 1048575) / 1048576 * 1048576 ))
[ "$cur" -lt "$want" ] && truncate -s "$want" build/bios.img
echo "build/bios.img: $(stat -c %s build/bios.img) bytes (kernel LBA $KERNEL_LBA, $KSEC sectors)"
