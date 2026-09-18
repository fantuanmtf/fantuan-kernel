#!/usr/bin/env bash
# fantuan-kernel — one-shot build & run in QEMU/OVMF.
# Usage: tools/run.sh [--arch x86_64|riscv64] [--graphics] [--broken]
#                     [--broken-shim] [--smm] [--no-smbios] [--two-fs]
#                     [--keys] [--shell-repair] [--nvme] [--bigcluster]
#                     [--liar] [--grub-regen]
#   --broken:      build the test disk with a missing EFI/BOOT/BOOTX64.EFI so
#                  the boot-repair fallback copy can be exercised (M7.5b).
#   --broken-shim: fallback AND shim missing — the NVRAM repair then has to
#                  delete the unfixable entry instead (M7.6).
#   --smm:    run on q35 with the SMM OVMF build (build/ovmf-smm/). Runtime
#             NVRAM writes (SetVariable, M7.6) only work in this mode — the
#             plain non-SMM OVMF build rejects them.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Architecture selection (prescan: --arch takes a separate argument).
ARCH="x86_64"
prev=""
for a in "$@"; do
  if [ "$prev" = "--arch" ]; then ARCH="$a"; fi
  prev="$a"
done
if [ "$ARCH" = "riscv64" ]; then
  # RISC-V: OpenSBI (QEMU -bios default) loads the ELF at 0x80200000.
  echo "[1/2] building kernel-riscv..."
  ./tools/build.sh --arch riscv64
  KERNEL="target/riscv64gc-unknown-none-elf/release/kernel-riscv"
  echo "[2/2] starting qemu-system-riscv64..."
  exec qemu-system-riscv64 -machine virt -bios default -nographic -kernel "$KERNEL"
fi

GRAPHICS=0
BROKEN=0
NOSHIM=0
SMM=0
SMBIOS=1
TWO_FS=0
KEYS=0
SHELL_REPAIR=0
NVME=0
BIGCLUSTER=0
LIAR=0
GRUB_REGEN=0
MONITOR=0
KBD_TEST=0
for a in "$@"; do
  case "$a" in
    --graphics)    GRAPHICS=1 ;;
    --broken)      BROKEN=1 ;;
    --broken-shim) BROKEN=1; NOSHIM=1 ;;
    --smm)         SMM=1 ;;
    --no-smbios)   SMBIOS=0 ;;
    --two-fs)      TWO_FS=1 ;;
    --keys)        KEYS=1 ;;
    --shell-repair) KEYS=1; SHELL_REPAIR=1 ;;
    --nvme)        NVME=1 ;;
    --bigcluster)  BIGCLUSTER=1 ;;
    --liar)        LIAR=1 ;;
    --grub-regen)  GRUB_REGEN=1 ;;
    --monitor)     MONITOR=1 ;;
    --kbd-test)    KBD_TEST=1 ;;
  esac
done

# SMBIOS test payload (M5.5): QEMU injects it via fw_cfg, OVMF publishes the
# table, the kernel parses it — the smoke greps for these exact strings.
# (Flags are appended to QEMU_ARGS below.)

echo "[1/4] building user program, kernel, bootloader..."
./tools/build.sh

mkdir -p build/esp/EFI/BOOT build/esp/fantuan
objcopy -O binary   target/x86_64-unknown-none/release/fantuan-kernel   build/esp/fantuan/kernel.bin
cp target/x86_64-unknown-uefi/release/fantuan-boot.efi   build/esp/EFI/BOOT/BOOTX64.EFI

echo "[3/4] preparing OVMF firmware..."
if [ "$SMM" = "1" ]; then
  # SMM build (M7.6): runtime NVRAM writes. Kept outside the repo; on Ubuntu
  # it ships in the ovmf-generic package as OVMF_CODE_4M.ms.fd + vars.
  SMM_DIR=""
  for d in build/ovmf-smm /usr/share/OVMF /usr/share/edk2/x64 /usr/share/edk2-ovmf /usr/share/edk2/OvmfX64; do
    if [ -f "$d/OVMF_CODE_4M.ms.fd" ]; then
      SMM_DIR="$d"
      break
    fi
  done
  if [ -z "$SMM_DIR" ]; then
    echo "error: SMM OVMF not found — put OVMF_CODE_4M.ms.fd + OVMF_VARS_4M.ms.fd" >&2
    echo "       into build/ovmf-smm/ (Ubuntu: ovmf-generic package)" >&2
    exit 1
  fi
  OVMF_CODE="$SMM_DIR/OVMF_CODE_4M.ms.fd"
  # Pair the SMM code with the NON-SecureBoot vars template: the ms build is
  # SB-enabled and would reject our unsigned loader; the plain template has
  # no enrolled keys, so SB stays off while SMM runtime variable writes work.
  SMM_VARS_TEMPLATE="$SMM_DIR/OVMF_VARS_4M.fd"
  if [ ! -f "$SMM_VARS_TEMPLATE" ]; then
    echo "error: SMM vars template OVMF_VARS_4M.fd missing next to the ms code" >&2
    exit 1
  fi
  [ -f build/OVMF_VARS.smm.fd ] || cp "$SMM_VARS_TEMPLATE" build/OVMF_VARS.smm.fd
  OVMF_VARS=build/OVMF_VARS.smm.fd
else
  OVMF_DIR=""
  for d in /usr/share/edk2/x64 /usr/share/OVMF /usr/share/ovmf /usr/share/edk2-ovmf /usr/share/edk2/OvmfX64; do
    if [ -f "$d/OVMF_CODE.4m.fd" ] || [ -f "$d/OVMF_CODE.fd" ]; then
      OVMF_DIR="$d"
      break
    fi
  done
  if [ -z "$OVMF_DIR" ]; then
    echo "error: OVMF firmware not found (install ovmf/edk2-ovmf)" >&2
    exit 1
  fi
  OVMF_CODE="$OVMF_DIR/OVMF_CODE.4m.fd"
  OVMF_VARS="$OVMF_DIR/OVMF_VARS.4m.fd"
  [ -f "$OVMF_CODE" ] || OVMF_CODE="$OVMF_DIR/OVMF_CODE.fd"
  [ -f "$OVMF_VARS" ] || OVMF_VARS="$OVMF_DIR/OVMF_VARS.fd"
  [ -f build/OVMF_VARS.fd ] || cp "$OVMF_VARS" build/OVMF_VARS.fd
  OVMF_VARS=build/OVMF_VARS.fd
fi

echo "[4/4] preparing AHCI test disk + starting QEMU..."
# GPT + FAT32 test disk for the C AHCI driver and the VFS (tools/mkdisk.py).
MKDISK_ARGS=""
if [ "$NOSHIM" = "1" ]; then
  MKDISK_ARGS="--broken-shim"
elif [ "$BROKEN" = "1" ]; then
  MKDISK_ARGS="--broken"
fi
if [ "$TWO_FS" = "1" ]; then
  MKDISK_ARGS="$MKDISK_ARGS --two-fs"
fi
if [ "$SHELL_REPAIR" = "1" ]; then
  MKDISK_ARGS="$MKDISK_ARGS --shell-repair"
elif [ "$KEYS" = "1" ]; then
  MKDISK_ARGS="$MKDISK_ARGS --keys"
fi
if [ "$BIGCLUSTER" = "1" ]; then
  MKDISK_ARGS="$MKDISK_ARGS --bigcluster"
fi
if [ "$LIAR" = "1" ]; then
  MKDISK_ARGS="$MKDISK_ARGS --liar"
fi
if [ "$GRUB_REGEN" = "1" ]; then
  MKDISK_ARGS="$MKDISK_ARGS --grub-regen"
fi
if [ "$KBD_TEST" = "1" ]; then
  MKDISK_ARGS="$MKDISK_ARGS --kbd-test"
fi
python3 tools/mkdisk.py $MKDISK_ARGS build/test.img
# Storage attachment: AHCI (reference) or NVMe (the same blk_ops table).
if [ "$NVME" = "1" ]; then
  AHCI_DEV="-drive file=build/test.img,format=raw,if=none,id=td0 -device nvme,drive=td0,serial=FANTUAN1"
else
  AHCI_DEV="-device ich9-ahci,id=sata -drive file=build/test.img,format=raw,if=none,id=td0 -device ide-hd,drive=td0,bus=sata.0"
fi

SERIAL_OPT="-nographic"
if [ "$GRAPHICS" = "1" ]; then SERIAL_OPT="-serial stdio"; fi
if [ "$MONITOR" = "1" ]; then
  # Keyboard-injection test path: a unix monitor socket while serial stays
  # on stdio (-nographic would multiplex the monitor onto the same stdio).
  rm -f build/mon.sock
  SERIAL_OPT="-display none -serial stdio"
fi

# Argument arrays (no line-continuation gymnastics).
QEMU_ARGS=(
  -m 512M
  # -cpu max exposes NX/SMEP/SMAP (and modern CPUID leaves) so the M8.3c
  # hardening path is actually exercised; the default qemu64 lacks SMEP/SMAP.
  -cpu max
  -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE"
  -drive "if=pflash,format=raw,file=$OVMF_VARS"
  -drive format=raw,file=fat:rw:build/esp
  -no-reboot -no-shutdown
)
# Storage attachment comes from $AHCI_DEV (AHCI by default, NVMe with --nvme).
QEMU_ARGS+=( $AHCI_DEV )
if [ "$SMBIOS" = "1" ]; then
  QEMU_ARGS+=(
    -smbios type=0,vendor=TESTCORP,version=1.2.3
    -smbios type=1,manufacturer=TESTVENDOR,product=TESTBOX
    -smbios type=4,manufacturer=TESTCPU
  )
fi
if [ "$SMM" = "1" ]; then
  # accel=kvm:tcg: use KVM when the host allows it, otherwise fall back to
  # TCG so the SMM phases run on CI boxes and machines without /dev/kvm.
  QEMU_ARGS=(
    -machine q35,smm=on,accel=kvm:tcg
    -global driver=cfi.pflash01,property=secure,value=on
    -global ICH9-LPC.disable_s3=1
    -global ICH9-LPC.disable_s4=1
    "${QEMU_ARGS[@]}"
  )
fi

# The monitor socket must be appended after QEMU_ARGS is fully built.
if [ "$MONITOR" = "1" ]; then
  QEMU_ARGS+=(-monitor "unix:build/mon.sock,server,nowait")
fi

exec qemu-system-x86_64 "${QEMU_ARGS[@]}" $SERIAL_OPT
