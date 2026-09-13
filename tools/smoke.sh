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
if grep -q "handshake ok" build/smoke.log && grep -q "beep: boot ok" build/smoke.log && grep -q "frame self-test ok" build/smoke.log && grep -q "demo tasks spawned" build/smoke.log && grep -q "task 3" build/smoke.log && grep -q "userland: hello" build/smoke.log && grep -q "userland: tid 5 exiting" build/smoke.log && grep -q "ahci: LBA0 read ok" build/smoke.log && grep -q "vfs: HELLO.TXT" build/smoke.log && grep -q "vfs: INFO.TXT => 1000" build/smoke.log && grep -q "esp: EFI/BOOT/BOOTX64.EFI" build/smoke.log && grep -q "fstab PARTUUID matches partition 1" build/smoke.log && grep -q "UUID matches grub.cfg" build/smoke.log && grep -q "nvram: BootCurrent" build/smoke.log && grep -q "BootOrder 5 entries" build/smoke.log && grep -q "repair: FIXED.TXT write+readback ok" build/smoke.log && grep -q "smbios: BIOS TESTCORP 1.2.3" build/smoke.log && grep -q "smbios: System TESTVENDOR TESTBOX" build/smoke.log && grep -q "smbios: DIMMs" build/smoke.log && grep -q "diskhealth: .*power-on" build/smoke.log && grep -q "fs: part 1 EFI System Partition" build/smoke.log && grep -q "bootloaders: EFI/BOOT/BOOTX64.EFI" build/smoke.log; then
  echo "SMOKE PASS"
  grep -E "handshake ok|frame allocator|frame self-test|syscall:|sched:|user: ELF|userland: hello|userland: tid 5|beep: boot ok" build/smoke.log | head -10
else
  echo "SMOKE FAIL — log tail:"
  tail -40 build/smoke.log
  exit 1
fi

# Second phase (M7.5b): a broken ESP (no EFI/BOOT/BOOTX64.EFI) must be
# repaired by copying the shim into place.
rm -f build/smoke-broken.log
timeout --signal=KILL 60 ./tools/run.sh --broken > build/smoke-broken.log 2>&1 || true
if grep -q "repair: FIXED.TXT write+readback ok" build/smoke-broken.log && grep -q "repair: copied EFI/ubuntu/shimx64.efi -> EFI/BOOT/BOOTX64.EFI" build/smoke-broken.log; then
  echo "SMOKE PASS (broken-ESP repair)"
  grep -E "repair: FIXED|fallback loader MISSING|repair: copied" build/smoke-broken.log
else
  echo "SMOKE FAIL (broken-ESP repair) — log tail:"
  tail -30 build/smoke-broken.log
  exit 1
fi

# M5.5 phase: the second partition carries ext4 magic only — the probe must
# identify it and must not mount it (mount contract), and the SMBIOS-free
# boot path must still come up.
rm -f build/smoke-probe.log build/smoke-nosmbios.log
timeout --signal=KILL 60 ./tools/run.sh --two-fs > build/smoke-probe.log 2>&1 || true
if grep -q "probe: part 2 ext4 identified, not mounted (v1 read-only probe-only)" build/smoke-probe.log \
   && grep -q "fs: part 1 EFI System Partition (mounted ro) part 2 ext4 (probe-only, v1 no read)" build/smoke-probe.log \
   && grep -q "bootloaders: EFI/BOOT/BOOTX64.EFI" build/smoke-probe.log; then
  echo "SMOKE PASS (fs probe: FAT32 mounted / ext4 probe-only)"
  grep -E "probe:|fs:|bootloaders:" build/smoke-probe.log | head -4
else
  echo "SMOKE FAIL (fs probe) — log tail:"
  tail -20 build/smoke-probe.log
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

# M7.7 phase: Secure Boot key inventory + the Setup-Mode enrollment path.
# The keyless OVMF vars template is a plain (non-auth) store, so the firmware
# refuses the write — the phase accepts either outcome and checks the report.
rm -f build/smoke-sb.log
timeout --signal=KILL 90 ./tools/run.sh --smm --keys > build/smoke-sb.log 2>&1 || true
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
  timeout --signal=KILL 90 ./tools/run.sh --smm --broken-shim > build/smoke-smm-a.log 2>&1 || true
  timeout --signal=KILL 90 ./tools/run.sh --smm --broken > build/smoke-smm-b.log 2>&1 || true
  timeout --signal=KILL 90 ./tools/run.sh --smm --broken > build/smoke-smm-c.log 2>&1 || true
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
