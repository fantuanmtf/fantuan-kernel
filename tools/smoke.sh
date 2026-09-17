#!/usr/bin/env bash
# Headless boot smoke test: run QEMU for a bounded time, expect the handshake.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build

# Reset the non-SMM vars store before each phase — repairs mutate NVRAM.
for d in /usr/share/edk2/x64 /usr/share/OVMF /usr/share/ovmf; do
  if [ -f "$d/OVMF_VARS.4m.fd" ]; then
    cp "$d/OVMF_VARS.4m.fd" build/OVMF_VARS.fd
    break
  fi
done

rm -f build/smoke.log
timeout --signal=KILL 60 ./tools/run.sh > build/smoke.log 2>&1 || true
if grep -q "handshake ok" build/smoke.log && grep -q "beep: boot ok" build/smoke.log && grep -q "frame self-test ok" build/smoke.log && grep -q "demo tasks spawned" build/smoke.log && grep -q "task 3" build/smoke.log && grep -q "userland: hello" build/smoke.log && grep -q "userland: tid 5 exiting" build/smoke.log && grep -q "ahci: LBA0 read ok" build/smoke.log && grep -q "vfs: HELLO.TXT" build/smoke.log && grep -q "vfs: INFO.TXT => 1000" build/smoke.log && grep -q "esp: EFI/BOOT/BOOTX64.EFI" build/smoke.log && grep -q "fstab PARTUUID matches partition 1" build/smoke.log && grep -q "UUID matches grub.cfg" build/smoke.log && grep -q "nvram: BootCurrent" build/smoke.log && grep -q "BootOrder 5 entries" build/smoke.log && ! grep -q "repair: FIXED" build/smoke.log && ! grep -q "repair: copied" build/smoke.log && ! grep -q "repair: created Boot" build/smoke.log && ! grep -q "repair: deleted stale" build/smoke.log && grep -q "smbios: BIOS TESTCORP 1.2.3" build/smoke.log && grep -q "smbios: System TESTVENDOR TESTBOX" build/smoke.log && grep -q "smbios: DIMMs" build/smoke.log && grep -q "diskhealth: .*power-on" build/smoke.log && grep -q "fs: part 1 EFI System Partition" build/smoke.log && grep -q "bootloaders: EFI/BOOT/BOOTX64.EFI" build/smoke.log; then
  echo "SMOKE PASS (boot path is read-only: no repair writes)"
  grep -E "handshake ok|frame allocator|frame self-test|syscall:|sched:|user: ELF|userland: hello|userland: tid 5|beep: boot ok" build/smoke.log | head -10
else
  echo "SMOKE FAIL — log tail:"
  tail -40 build/smoke.log
  exit 1
fi

# Second phase (M7.5b): a broken ESP (no EFI/BOOT/BOOTX64.EFI) must be
# repaired by copying the shim into place.
rm -f build/smoke-broken.log
# The repair itself is consent-gated now: the shell's autorun script runs
# `grub-fix repair` and answers YES, which is what enables repair mode.
timeout --signal=KILL 90 ./tools/run.sh --broken --shell-repair > build/smoke-broken.log 2>&1 || true
if grep -q "confirm> YES" build/smoke-broken.log \
   && grep -q "repair: repair mode ON" build/smoke-broken.log \
   && grep -q "repair: FIXED.TXT write+readback ok" build/smoke-broken.log \
   && grep -q "repair: copied EFI/ubuntu/shimx64.efi -> EFI/BOOT/BOOTX64.EFI (21 bytes, verified)" build/smoke-broken.log; then
  echo "SMOKE PASS (broken-ESP repair via shell YES confirmation)"
  grep -aE "confirm>|repair mode ON|repair: FIXED|fallback loader MISSING|repair: copied" build/smoke-broken.log
else
  echo "SMOKE FAIL (broken-ESP repair) — log tail:"
  tail -30 build/smoke-broken.log
  exit 1
fi

# M6.5 phase: the second partition is a real (hand-built) ext4 root — the
# driver must mount it ro, read the real /etc/fstab from it, cross-check its
# UUID against grub.cfg, and serve `cat /etc/fstab` through the shell (the
# --two-fs autorun adds that command). The XFS magic on part 3 must stay
# probe-only.
rm -f build/smoke-ext4.log build/smoke-nosmbios.log
timeout --signal=KILL 90 ./tools/run.sh --two-fs --keys > build/smoke-ext4.log 2>&1 || true
if grep -q "ext4: mounted ro — uuid 12345678-1234-1234-1234-123456789abc, 64 blocks of 1024 B, 16 inodes" build/smoke-ext4.log \
   && grep -q "ext4: mounted ro at /mnt/root0 (part 2)" build/smoke-ext4.log \
   && grep -q "probe: part 3 XFS identified, not mounted (probe-only)" build/smoke-ext4.log \
   && grep -q "fs: part 1 EFI System Partition (mounted ro) part 2 ext4 (mounted ro) part 3 XFS (probe-only)" build/smoke-ext4.log \
   && grep -q "bootrepair: fstab read from the ext4 root (/etc/fstab, 147 bytes)" build/smoke-ext4.log \
   && grep -q "bootrepair: grub.cfg search.fs_uuid matches the ext4 root UUID (consistent)" build/smoke-ext4.log \
   && grep -q "cat: 147 bytes" build/smoke-ext4.log \
   && grep -q "UUID=12345678-1234-1234-1234-123456789abc / ext4" build/smoke-ext4.log; then
  echo "SMOKE PASS (M6.5: ext4 mounted ro, real /etc/fstab, XFS probe-only)"
  grep -aE "ext4: mounted|probe: part 3|fstab read|cat: 147" build/smoke-ext4.log | head -6
else
  echo "SMOKE FAIL (M6.5 ext4) — log tail:"
  tail -25 build/smoke-ext4.log
  exit 1
fi
timeout --signal=KILL 60 ./tools/run.sh --no-smbios > build/smoke-nosmbios.log 2>&1 || true
if grep -q "handshake ok" build/smoke-nosmbios.log && grep -q "smbios:" build/smoke-nosmbios.log; then
  echo "SMOKE PASS (no -smbios overrides: firmware defaults parsed)"
else
  echo "SMOKE FAIL (no -smbios) — log tail:"
  tail -20 build/smoke-nosmbios.log
  exit 1
fi

# Audit phase (P2): a crafted ESP whose HELLO.TXT claims 4096 bytes while the
# file holds 35 must not panic the boot path — read_file truncates to the
# caller's buffer and the boot continues.
rm -f build/smoke-liar.log
timeout --signal=KILL 60 ./tools/run.sh --liar > build/smoke-liar.log 2>&1 || true
if grep -q "handshake ok" build/smoke-liar.log \
   && grep -q "vfs: HELLO.TXT =>" build/smoke-liar.log \
   && grep -q "vfs: INFO.TXT => 1000" build/smoke-liar.log; then
  echo "SMOKE PASS (crafted FAT: oversized file entry truncated, boot completes)"
  grep -aE "vfs: (HELLO|INFO)" build/smoke-liar.log
else
  echo "SMOKE FAIL (crafted FAT size lie) — log tail:"
  tail -20 build/smoke-liar.log
  exit 1
fi

# Audit phase (P2): 4 KiB clusters + the 21-byte fallback copy — the short
# final chunk must zero-pad the rest of its cluster instead of indexing past
# the buffer.
rm -f build/smoke-bigcluster.log
timeout --signal=KILL 90 ./tools/run.sh --bigcluster --broken --shell-repair > build/smoke-bigcluster.log 2>&1 || true
if grep -q "repair: copied EFI/ubuntu/shimx64.efi -> EFI/BOOT/BOOTX64.EFI (21 bytes, verified)" build/smoke-bigcluster.log \
   && grep -q "vfs: INFO.TXT => 1000 bytes read, all-X true" build/smoke-bigcluster.log \
   && grep -q "cat: 21 bytes" build/smoke-bigcluster.log; then
  echo "SMOKE PASS (4 KiB clusters: short-data cluster write zero-pads)"
  grep -aE "repair: copied|vfs: INFO|cat: " build/smoke-bigcluster.log | head -4
else
  echo "SMOKE FAIL (4 KiB clusters) — log tail:"
  tail -25 build/smoke-bigcluster.log
  exit 1
fi

# §10 shell phase: the ESP carries an autorun script (EFI/fantuan/SHELL.CMD),
# so the read-only commands and the confirmation-gated repair path are
# exercised deterministically — no timing-dependent serial injection.
rm -f build/smoke-shell.log build/smoke-shell-repair.log
# The idle notice needs ~30 s after the autorun scan finishes, and a TCG boot
# can take most of the first minute: budget 150 s.
timeout --signal=KILL 150 ./tools/run.sh --keys --broken > build/smoke-shell.log 2>&1 || true
if grep -q "shell: autorun 8 command(s)" build/smoke-shell.log \
   && grep -q "shell> help" build/smoke-shell.log \
   && grep -q "hwdiag      re-run hardware" build/smoke-shell.log \
   && grep -q "part 1: EFI System Partition" build/smoke-shell.log \
   && grep -q "cat: 35 bytes" build/smoke-shell.log \
   && grep -q "scan: done" build/smoke-shell.log \
   && grep -q "idle on serial" build/smoke-shell.log; then
  echo "SMOKE PASS (shell: autorun commands + surface scan + idle notice)"
  grep -aE "shell: (ready|autorun|idle)|cat: |scan: done" build/smoke-shell.log | head -6
else
  echo "SMOKE FAIL (shell autorun) — log tail:"
  tail -25 build/smoke-shell.log
  exit 1
fi
timeout --signal=KILL 90 ./tools/run.sh --shell-repair --broken > build/smoke-shell-repair.log 2>&1 || true
if grep -q "WARNING: repair mode enables disk writes" build/smoke-shell-repair.log \
   && grep -q "confirm> YES" build/smoke-shell-repair.log \
   && grep -q "repair: repair mode ON" build/smoke-shell-repair.log \
   && grep -q "repair: FIXED.TXT write+readback ok" build/smoke-shell-repair.log \
   && grep -q "cat: 21 bytes" build/smoke-shell-repair.log; then
  echo "SMOKE PASS (shell: confirmation-gated repair + cat of the repaired fallback)"
  grep -aE "WARNING: repair|confirm>|repair mode ON|cat: " build/smoke-shell-repair.log | head -4
else
  echo "SMOKE FAIL (shell repair path) — log tail:"
  tail -25 build/smoke-shell-repair.log
  exit 1
fi

# NVMe phase (M8): the same stack over a different transport — the driver
# registry picks the NVMe ops table and everything above it is unchanged.
rm -f build/smoke-nvme.log
timeout --signal=KILL 75 ./tools/run.sh --nvme > build/smoke-nvme.log 2>&1 || true
if grep -q "blk: nvme registered" build/smoke-nvme.log \
   && grep -q "storage: QEMU NVMe Ctrl" build/smoke-nvme.log \
   && grep -q "diskhealth: QEMU NVMe Ctrl  SSD" build/smoke-nvme.log \
   && grep -q "vfs: HELLO.TXT" build/smoke-nvme.log \
   && grep -q "vfs: INFO.TXT => 1000" build/smoke-nvme.log \
   && grep -q "esp: EFI/BOOT/BOOTX64.EFI" build/smoke-nvme.log; then
  echo "SMOKE PASS (NVMe: controller + VFS + SMART + boot repair)"
  grep -aE "nvme:|blk: |storage: QEMU NVMe|diskhealth: QEMU NVMe" build/smoke-nvme.log | head -5
else
  echo "SMOKE FAIL (NVMe) — log tail:"
  tail -25 build/smoke-nvme.log
  exit 1
fi

# M7.7 phase: Secure Boot key inventory + the Setup-Mode enrollment path.
# The keyless OVMF vars template is a plain (non-auth) store, so the firmware
# refuses the write — the phase accepts either outcome and checks the report.
rm -f build/smoke-sb.log
timeout --signal=KILL 90 ./tools/run.sh --smm --shell-repair > build/smoke-sb.log 2>&1 || true
if grep -q "nvram: SetupMode" build/smoke-sb.log \
   && grep -q "secureboot: PK absent" build/smoke-sb.log \
   && grep -q "secureboot: SetupMode ACTIVE" build/smoke-sb.log \
   && grep -q "enrolling PK from EFI/fantuan/PK.cer" build/smoke-sb.log; then
  if grep -q "secureboot: PK write sts=0x0" build/smoke-sb.log; then
    echo "SMOKE PASS (Secure Boot: platform key enrolled)"
    grep -E "secureboot: (PK write|WARNING)" build/smoke-sb.log
  else
    echo "SMOKE PASS (Secure Boot: enrollment attempted; firmware store rejects authenticated variables)"
    grep -E "secureboot: (PK write|enrollment refused)" build/smoke-sb.log
  fi
else
  echo "SMOKE FAIL (Secure Boot keys) — log tail:"
  tail -20 build/smoke-sb.log
  exit 1
fi

# Third phase (M7.6): NVRAM repair via SetVariable — three boots on the
# persistent SMM vars store (template entry Boot0002 is a whole-disk entry
# on the ESP's AHCI controller):
#   A: broken + no shim -> the entry is unfixable -> drop from BootOrder + delete
#   B: broken           -> no entry covers the ESP anymore -> create Boot####
#   C: broken           -> the created entry is healed by the fallback copy -> kept
if [ -f build/ovmf-smm/OVMF_CODE_4M.ms.fd ] && [ -f build/ovmf-smm/OVMF_VARS_4M.fd ]; then
  cp build/ovmf-smm/OVMF_VARS_4M.fd build/OVMF_VARS.smm.fd
  rm -f build/smoke-smm-a.log build/smoke-smm-b.log build/smoke-smm-c.log
  timeout --signal=KILL 90 ./tools/run.sh --smm --broken-shim --shell-repair > build/smoke-smm-a.log 2>&1 || true
  timeout --signal=KILL 90 ./tools/run.sh --smm --broken --shell-repair > build/smoke-smm-b.log 2>&1 || true
  timeout --signal=KILL 90 ./tools/run.sh --smm --broken --shell-repair > build/smoke-smm-c.log 2>&1 || true
  A_OK=$(grep -c "repair: deleted stale Boot" build/smoke-smm-a.log)
  B_OK=$(grep -c "repair: created Boot" build/smoke-smm-b.log)
  C_OK=$(grep -c "repair: BootOrder unchanged" build/smoke-smm-c.log)
  if [ "$A_OK" -ge 1 ] && [ "$B_OK" -ge 1 ] && [ "$C_OK" -ge 1 ] \
     && grep -q "BootOrder.*SetVariable sts=0x0, verified true" build/smoke-smm-a.log \
     && grep -q "BootOrder.*SetVariable sts=0x0, verified true" build/smoke-smm-b.log; then
    echo "SMOKE PASS (NVRAM repair: delete/create/keep)"
    grep -hE "repair: created Boot|repair: deleted stale|repair: BootOrder" build/smoke-smm-a.log build/smoke-smm-b.log build/smoke-smm-c.log
  else
    echo "SMOKE FAIL (NVRAM repair) — A=$A_OK B=$B_OK C=$C_OK, log tails:"
    tail -15 build/smoke-smm-a.log
    tail -15 build/smoke-smm-b.log
    tail -15 build/smoke-smm-c.log
    exit 1
  fi
else
  echo "SMOKE SKIP (NVRAM repair): SMM OVMF not in build/ovmf-smm/"
fi
