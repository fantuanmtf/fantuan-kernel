#!/usr/bin/env bash
# M10 BIOS smoke: builds both BIOS images and boots them under SeaBIOS with
# the *default* QEMU CPU (no SMEP/SMAP), exercising the old-machine paths.
#   phase 1: x86_64 kernel (M10-3) -> tasks, userland, shell
#   phase 2: i686 kernel (M10-4)   -> handoff, memmap, frame allocator,
#            PIO ATA primary master + shared VFS on the test disk
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"

run() { # image log timeout
  timeout --signal=KILL "$3" qemu-system-x86_64 -machine pc -m 512M \
    -drive format=raw,file="$1" -nographic -no-reboot \
    < /dev/null > "$2" 2>&1 || true
}

# i686 VFS phase: the test disk is the primary master (the delivered PIO
# driver), the boot image moves to the primary slave with bootindex=1.
run_disk() { # boot_image test_disk log timeout
  timeout --signal=KILL "$4" qemu-system-x86_64 -machine pc -m 512M \
    -drive file="$1",format=raw,if=none,id=boot0 \
    -device ide-hd,drive=boot0,bus=ide.0,unit=1,bootindex=1 \
    -drive file="$2",format=raw,if=none,id=td0 \
    -device ide-hd,drive=td0,bus=ide.0 \
    -nographic -no-reboot < /dev/null > "$3" 2>&1 || true
}

echo "[phase 1] x86_64 BIOS kernel..."
./tools/build-bios.sh > /dev/null
LOG="build/smoke-bios.log"
rm -f "$LOG"
run build/bios.img "$LOG" 60
if grep -q "fantuan-bios stage2 (M10-3)" "$LOG" \
   && grep -q "memmap: descriptors=" "$LOG" \
   && grep -q "handshake ok: magic=0x46544e46 version=2" "$LOG" \
   && grep -q "console: none (serial-only; GOP unavailable)" "$LOG" \
   && grep -q "mm: frame allocator ready" "$LOG" \
   && grep -q "mm: reclaimed 13 bootloader table pages" "$LOG" \
   && grep -q "mmu: nx true" "$LOG" \
   && grep -q "userland: hello from tid" "$LOG" \
   && grep -q "shell: ready" "$LOG"; then
  echo "SMOKE PASS (bios x86_64: MBR -> LBA -> E820 -> BootInfo -> ATA -> long mode -> kernel/tasks/shell)"
else
  echo "SMOKE FAIL (bios x86_64) — log tail:"
  tail -25 "$LOG"
  exit 1
fi

echo "[phase 2] i686 BIOS kernel + PIO ATA VFS on the test disk..."
./tools/build-bios.sh --arch i686 --slave > /dev/null
python3 tools/mkdisk.py build/test.img > /dev/null
LOG2="build/smoke-bios-i686.log"
rm -f "$LOG2"
run_disk build/bios-i686.img build/test.img "$LOG2" 45
if grep -q "fantuan v0.0.1 (i686) - BIOS handoff" "$LOG2" \
   && grep -q "handshake ok: arch=3 kernel_base=0x1000000 stack_top=0x80000" "$LOG2" \
   && grep -q "memmap: 6 descriptors" "$LOG2" \
   && grep -q "base=0x100000 pages=130784 type=7" "$LOG2" \
   && grep -q "mm: frame allocator ready: 510 MiB usable" "$LOG2" \
   && grep -q "mm: frame self-test ok (via PHYS_OFFSET alias)" "$LOG2" \
   && grep -q "gdt: ring0/ring3 + TSS loaded" "$LOG2" \
   && grep -q "idt: 48 vectors" "$LOG2" \
   && grep -q "exc 3 (breakpoint) err=0x0" "$LOG2" \
   && grep -q "demo: #BP handled and resumed" "$LOG2" \
   && grep -q "timer: 500 ticks, exceptions=2" "$LOG2" \
   && grep -q "task 1 (tid 1): hello 1" "$LOG2" \
   && grep -q "task 2 (tid 2): hello 2" "$LOG2" \
   && grep -q "task 1: quiet (scheduler keeps rotating)" "$LOG2" \
   && grep -q "user: elf32 userland spawned as tid 3" "$LOG2" \
   && grep -q "userland: hello from tid 3" "$LOG2" \
   && grep -q "sched: reaped tid 3" "$LOG2" \
   && grep -q "user: fault stub spawned as tid 4" "$LOG2" \
   && grep -q "user fault: tid 4 killed (vec 6 eip 0x400000)" "$LOG2" \
   && grep -q "sched: reaped tid 4" "$LOG2" \
   && grep -q "i686: M10-4b2a interrupts complete" "$LOG2" \
   && grep -q "ata: primary master ready" "$LOG2" \
   && grep -q "vfs: mounted FAT32 at /mnt/disk0" "$LOG2" \
   && grep -q "vfs: HELLO.TXT =>" "$LOG2" \
   && grep -q "vfs: INFO.TXT => 1000 bytes read, all-X true" "$LOG2" \
   && grep -q "fs: part 1 EFI System Partition" "$LOG2" \
   && grep -q "esp: EFI/BOOT/BOOTX64.EFI" "$LOG2"; then
  echo "SMOKE PASS (bios i686: BootInfo -> paging -> allocator -> IDT/PIC/PIT -> scheduler -> ring 3 ELF32 / int 0x80 -> PIO ATA VFS)"
  grep -aE "handshake ok|memmap: 6|base=0x100000|mm: frame|idt:|exc 3|demo:|timer: 500|i686:" "$LOG2" | head -10 || true
else
  echo "SMOKE FAIL (bios i686) — log tail:"
  tail -25 "$LOG2"
  exit 1
fi
