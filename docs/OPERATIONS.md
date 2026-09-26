# Operations Guide

Day-2 operations for the fantuan-kernel test rigs: running the suites,
driving QEMU, reading logs, troubleshooting and releasing. For the shell
itself see [USAGE.md](USAGE.md); for builds see [BUILD.md](BUILD.md).

## 1. Automation at a glance

| Script | Purpose | Typical duration |
|---|---|---|
| `tools/build.sh [--arch riscv64]` | build all artifacts for one arch | < 1 min |
| `tools/smoke.sh` | full x86 acceptance suite (13 phases) | ~25 min |
| `tools/smoke-config.sh` | profile invariants, command-string gating, budget, incrementality | ~3 min |
| `tools/smoke-bash.sh` | P3 bash gate: -c/interactive transcripts, arithmetic/variables/functions, pipes/redirects/`$(...)`, SIGINT, reaps, `sh`=bash | ~4 min |
| `tools/smoke-dash.sh` | P2 dash gate (dash selected explicitly): -c/interactive transcripts, pipes/redirects/scripts, SIGINT, reaps | ~2 min |
| `tools/smoke-posix.sh` | P1 libc/ELF-loader gate (hello, brk, tmpfs, pipe) | ~1 min |
| `tools/smoke-net.sh` | M11 offline network gate (loopback/SLIRP/DNS/TLS/UDP) | ~8 min |
| `tools/smoke-imager.sh` | M12 `clone` gate: verified round trip, size/YES gates, read-only branch | ~2 min |
| `tools/smoke-imager-bad.sh` | M12-3 bad-sector gate: default abort, `--continue` zero-fill, report ranges/counts, `--quick` | ~1 min |
| `tools/smoke-ntfs.sh` | M12-4/M12-5 NTFS gate: `/mnt/win0` mount/facts, listing equality, resident + fragmented hashes, corrupt-record rejection, no-write | ~1 min |
| `tools/smoke-gpu.sh` | M12-6 GPU/PCI gate: std/cirrus/virtio identity + BAR lines, 64-bit aperture mapped ro, `pcie n/a`, no ACPI TZ, shell `gpu` | ~5 min |
| `tools/smoke-graphics.sh` | M13-1/M13-2 framebuffer gate: non-blank GOP + VBE screendumps, damage self-test, console damage containment, i686 serial parity | ~5 min |
| `tools/smoke-input.sh` | M13-3 input gate: injected PS/2 pointer/keyboard events reach the ring, demo cursor + early stop, screendump containment | ~2 min |
| `tools/smoke-kms.sh` | M13-4 KMS gate: dumb buffers + ADDFB/SETCRTC/PAGE_FLIP, flip-completion events, geometry-mismatch negative, cleanup, screendump inside the fb geometry | ~5 min |
| `tools/smoke-bios.sh` | legacy BIOS chain: x86_64 + i686 (2 phases) | ~2 min |
| `tools/smoke-riscv.sh` | riscv acceptance suite (3 phases) | ~4 min |
| `tools/smoke-aarch64.sh` | aarch64 direct FDT + virtio-net/TLS offline gate (2 phases) | ~4 min |
| `tools/smoke-iso.sh` | hybrid ISO: BIOS El Torito + UEFI 0xEF (2 phases) | ~3 min |
| `tools/run.sh [flags]` | single interactive/scripted boot | until you quit |
| `tools/kbd_test.sh` | QEMU-monitor keyboard injection (x86) | ~1 min |

All smoke scripts bound every QEMU run with `timeout --signal=KILL` and
exit non-zero on the first failing phase. Logs land in `build/*.log`.

## 2. The x86 suite (`tools/smoke.sh`, 13 phases)

The suite selects an explicit richer profile before booting
(`minimal` + `RESCUE_REPAIR` + `VIRT` + `SMBIOS` + `GRAPHICS`): it asserts
the storage diagnostics, the M12-6 GPU/PCI block and the rescue/VFS shell
commands that the default minimal kernel gates out (C5).

| # | Phase (PASS string suffix) | What it proves |
|---|---|---|
| 1 | boot path is read-only: no repair writes | normal boot + negative write greps + the `gpu:` identity/BAR/thermal block |
| 2 | broken-ESP repair via shell YES confirmation | consent-gated fallback copy |
| 3 | ext4 root, real fstab, /boot inventory, systemd/UKI | read-only root stack |
| 4 | no -smbios overrides: firmware defaults parsed | SMBIOS fallback |
| 5 | grub-fix install — backup, regenerate, publish, verify | config regeneration |
| 6 | crafted FAT: oversized file entry truncated | hostile-input robustness |
| 7 | 4 KiB clusters: short-data cluster write zero-pads | FAT write correctness |
| 8 | shell: autorun commands + surface scan + crypto KATs + idle notice | interactive shell; the autorun also re-runs `gpu` |
| 9 | shell: confirmation-gated repair + cat of the repaired fallback | repair round-trip |
| 10 | NVMe: controller + VFS + SMART + boot repair | second block transport |
| 11 | Secure Boot: platform key enrolled | Setup Mode enrollment |
| 12 | Secure Boot: .auth verified, firmware store rejects authenticated variables | `.auth` structure/self-verify |
| 13 | NVRAM repair: delete/create/keep | three-boot SMM SetVariable sequence |

Phases 2, 9, 11–13 need the SMM OVMF build in `build/ovmf-smm/`
(`OVMF_CODE_4M.ms.fd` + `OVMF_VARS_4M.fd`); the script copies the vars
template before phases 11–13 because NVRAM repairs mutate it.

## 3. Boot matrix (verified paths)

| Boot path | Firmware / loader | Suite | What is verified |
|---|---|---|---|
| x86_64 UEFI | OVMF + `boot/` (GOP + serial) | `smoke.sh` 13/13 | VFS, diagnostics, boot repair, NVRAM (SMM), userland, shell; `-cpu max` exercises SMEP/SMAP |
| x86_64 MBR/BIOS | SeaBIOS + `boot-bios/` stage1/stage2 | `smoke-bios.sh` phase 1 | E820 -> BootInfo `arch=3` -> long mode -> ATA PIO kernel load -> tasks + shell on serial-only |
| i686 BIOS | SeaBIOS + the 32-bit stage2 variant | `smoke-bios.sh` phase 2 | 32-bit handoff, PSE paging, frame allocator, IDT/PIC/PIT, scheduler, ELF32 ring 3 via `int 0x80`, PIO ATA + shared VFS (read-only) |
| i686 BIOS + data disk | as above, kernel on the primary **slave** (`build-bios.sh --arch i686 --slave`), `build/test.img` on the primary master | `smoke-bios.sh` phase 2 | the PIO ATA driver reads the delivered test disk while the firmware boots the slave image |
| riscv64 | OpenSBI `fw_dynamic`, QEMU `virt` (no UEFI involved) | `smoke-riscv.sh` 3/3 | boot/Sv39/traps/SBI timer, U-mode + fault kill/reap, virtio-mmio, VFS/boot-repair, shell, repair YES/NO gate |
| aarch64 direct | QEMU `virt` raw `Image` boot (no firmware; DTB in x0) | `smoke-aarch64.sh` 2/2 | phase R9a: boot/PL011/FDT, 4K-granule MMU + PHYS_OFFSET direct map, GICv2 + generic timer at 100 Hz, BRK-resume exception demo, kernel tasks, shell; phase R9b: virtio-net MMIO on SLIRP (modern transport), the rump self-tests, DHCP + IPv4/TCP, HTTP/DNS/ping/wget, pinned-CA HTTPS (mbedTLS KATs) and the host UDP echo, plus the shell tool transcripts |
| Hybrid ISO BIOS | SeaBIOS `-cdrom`, El Torito no-emulation preload | `smoke-iso.sh` phase 1 | the x86_64 kernel reaches the shell from the ISO; the CD chain copies the kernel from the firmware preload instead of ATA |
| Hybrid ISO UEFI | OVMF `-cdrom`, platform id 0xEF FAT ESP | `smoke-iso.sh` phase 2 | OVMF mounts the 0xEF FAT image and boots the same kernel to VFS + shell |
| VBE console (i686) | SeaBIOS `-vga std` + stage2 VBE 2.0 mode set | `smoke-bios.sh` phase 2 asserts `fb: 1024x768x32` + `fb: console up`; the W5 checkpoint added a headless screendump decode and the `-vga none` fallback run | 1024x768x32 text console with serial mirroring; without VBE the kernel logs `fb: unavailable (serial console)` and continues on serial |

Known limitations across the matrix:

- **i686 direct-map cap**: only the first 1 GiB of physical RAM is aliased
  (3G/1G split); the frame allocator caps usable RAM and logs the
  truncation (`510 MiB usable` in the 512 MiB QEMU run). No PAE.
- **i686 is read-only**: the PIO ATA block layer has no write path, so
  repair commands are unavailable there (no NVRAM on BIOS either) and the
  shared imager's first write fails with `destination is read-only on this
  build`. i686 has no shell yet, so there is no interactive clone
  transcript; the rescue/IMAGER build links the branch (W-a).
- **i686 has no shell yet**: the kernel runs the demo/userland sequence
  and halts; VFS coverage is boot-time assertion, not shell.
- **ISO is CD-ROM only**: no isohybrid MBR and no USB `dd` support; the
  builder enforces the 1 GiB budget and `smoke-iso.sh` re-checks it.
- **VBE is QEMU-only**: physical-firmware VBE is untested (SeaBIOS `-vga
  std` is the only environment exercised).
- **riscv has no UEFI**: OpenSBI is the boot path; Runtime Services are
  absent and NVRAM repair degrades honestly.
- **aarch64 is console + network**: the direct-FDT boot has no storage
  transport and no user mode yet (storage/user mode land with M12/M14); the
  shared VFS resolves through stub `blk_read`/`blk_write` returning -1. The
  network phase needs a QEMU binary with the SLIRP `user` backend (the
  script prints SKIP when it is missing) and is virtio-net MMIO only.
- **aarch64 UEFI/AAVMF is not implemented**: only the direct-FDT path is
  supported in v0.0.4; the loader port (aarch64 UEFI application, exact
  load address, cache/MMU-off trampoline, DTB from the FDT config table) is
  deferred to M14.

## 4. The riscv suite (`tools/smoke-riscv.sh`, 3 phases)

| Phase | What it proves |
|---|---|
| A (read-only) | boot, Sv39, FDT report, SBI timer, kernel + user tasks, fault kill/reap, virtio-blk, FAT32/ext4/probe, boot-repair diagnosis, UART shell (`diskhealth`, `cat`), and **no repair writes** |
| B (repair YES) | `grub-fix repair` + `YES` over virtio-blk: FIXED.TXT write+readback, fallback shim copy verified |
| C (repair NO) | the confirmation gate aborts with nothing written |

Phase A drives the shell by piping commands into QEMU's serial console
(`sleep 5; printf 'diskhealth\ncat HELLO.TXT\n'; sleep ...`) — the pipe
stays open so QEMU does not see EOF. The same technique drives phases B/C.
The script selects the `rescue` profile because the default minimal kernel
does not register those commands (C5).

## 4.1 The aarch64 suite (`tools/smoke-aarch64.sh`, 2 phases)

**Phase R9a** boots the raw `Image` on QEMU `virt` with `-cpu cortex-a72`
and `-machine virt,gic-version=2` (QEMU's Linux-compatible raw-image
protocol is what passes the DTB in x0) and asserts the boot banner, the
FDT memory/model/pl011 report, the frame allocator and direct-map markers,
GICv2 + the 100 Hz timer, the `brk #0` resume, the two demo tasks, the
`help`/`bootinfo` shell transcripts, no unexpected traps, no panic and no
heartbeat after `shell: ready`. It selects the minimal profile.

**Phase R9b** builds the `tls` profile and boots the same machine with
`-device virtio-net-device` on QEMU user networking (SLIRP) plus
`-global virtio-mmio.force-legacy=false` (QEMU `virt` has no PCI, and the
driver binds the modern version-2 MMIO transport). The host fixtures from
`tools/net_fixtures.py` (HTTP 18080, DNS 5353, TLS 18443 with a per-run
pinned CA, UDP 18082) run on `127.0.0.1`, reachable from the guest as
`10.0.2.2`; `FANTUAN_NET_FIXTURES=1` enables the R8 boot checks. The script
asserts `net: virtio-net up mac=...`, the DHCP lease, the rump self-tests,
the TCP transfer/retransmit markers, HTTP/DNS/ping/wget, the TLS KATs,
pinned-CA HTTPS, the host UDP echo, the external skip, and the shell
transcripts for `nslookup`/`ping`/`wget` fed over the serial console. It
prints SKIP (not FAIL) when openssl or the QEMU SLIRP backend is missing.

```sh
# the exact QEMU invocation run.sh uses (raw Image, GICv2, one A72):
qemu-system-aarch64 -machine virt,gic-version=2 -cpu cortex-a72 \
  -nographic -m 512M -nic none -kernel build/kernel-aarch64.bin

# the R9b network phase adds:
#   -global virtio-mmio.force-legacy=false \
#   -netdev user,id=n0 -device virtio-net-device,netdev=n0

tools/run.sh --arch aarch64            # interactive: build + QEMU virt
tools/run.sh --arch aarch64 --net      # interactive: + virtio-net/SLIRP
tools/smoke-aarch64.sh                 # bounded acceptance run (PASS/SKIP)
```

## 5. `tools/run.sh` flags

| Flag | Effect |
|---|---|
| `--arch x86_64\|riscv64\|aarch64` | select the platform (default x86_64) |
| `--graphics` | x86: show the GOP window instead of `-nographic` |
| `--net` | attach the kernel-net NIC on QEMU user networking (x86_64: e1000; aarch64: virtio-net-device MMIO with `virtio-mmio.force-legacy=false`); without it the NIC is disabled |
| `--broken` / `--broken-shim` | build the ESP without fallback / without shim |
| `--two-fs` | add the ext4 root + XFS probe fixture to the disk |
| `--keys` | Secure Boot certs + autorun shell script |
| `--shell-repair` | autorun runs `grub-fix repair` and answers YES |
| `--grub-regen` | autorun runs `grub-fix install` (implies `--two-fs`) |
| `--imager` | M12 `clone` fixtures on one AHCI controller: test disk (blk0) + pattern source (blk1) + larger/smaller empty destinations (blk2/blk3); autorun runs the gate transcript |
| `--imager-bad` | M12-3 bad-sector fixtures: blk1 pattern with QEMU blkdebug read errors, blk2 empty destination, blk3 clean pattern; autorun runs abort/`--continue`/`--quick` (ranges default `100:4,700:2`, override with `BADCLUSTERS`) |
| `--ntfs` | M12-4/M12-5 NTFS fixture: `mkdisk.py --ntfs` adds the hand-built read-only NTFS volume (partition 2) and the `/mnt/win0` autorun (listings, reads, corrupt-record and write-refusal probes) |
| `--vga std\|cirrus\|virtio\|none` | x86 display model for the M12-6 GPU probe (default `std`); `smoke-gpu.sh` boots std/cirrus/virtio |
| `--smm` | x86 on q35 with SMM OVMF (required for SetVariable) |
| `--no-smbios` | boot without `-smbios` overrides (firmware defaults) |
| `--nvme` | attach the disk as NVMe instead of AHCI |
| `--bigcluster` | 4 KiB-cluster FAT fixture |
| `--liar` | crafted FAT with an oversized file entry |
| `--disk` (riscv) | attach `build/test.img` as virtio-blk |
| `--keys`/`--shell-repair` (riscv) | forward the autorun fixture to mkdisk |

x86 `run.sh` also rebuilds the ESP in `build/esp/` and objcopies the kernel
to `build/esp/fantuan/kernel.bin` on every run.

## 5.1 Manual (interactive) boot testing

| Path | Command | What you get |
|---|---|---|
| UEFI x86_64 | `tools/run.sh` | full profile stack: VFS, diagnostics, userland, interactive shell on serial (the default minimal boot has the core builtins only) |
| UEFI x86_64 GUI | `tools/run.sh --graphics` | the same plus a GOP window (serial stays on stdio) |
| BIOS x86_64 | `tools/run-bios.sh` | the M10 BIOS chain and an interactive shell; the image has no partitions, so `lsos`/`cat` report no filesystems |
| BIOS i686 | `tools/run-bios.sh --arch i686` | the 32-bit bring-up lines (handoff, memmap, frame allocator, interrupts, ELF32 user tasks, VBE console); no disk, so `ata: no primary master (VFS skipped)`, and no shell yet |
| BIOS i686 + VFS | `tools/build-bios.sh --arch i686 --slave` + the QEMU layout in `smoke-bios.sh` | the same bring-up with `build/test.img` on the primary master (read-only VFS) |
| Hybrid ISO | `tools/build-iso.sh` then `qemu-system-x86_64 -cdrom build/fantuan.iso -nographic` | the same kernel via El Torito on BIOS; OVMF boots the 0xEF ESP path (`smoke-iso.sh` shows the exact invocation) |
| RISC-V | `tools/run.sh --arch riscv64 --disk --two-fs` | OpenSBI + virtio-blk + VFS/shell |
| aarch64 | `tools/run.sh --arch aarch64 [--net]` | QEMU `virt` raw-Image direct FDT boot: PL011, 4K-granule MMU + direct map, GICv2 + 100 Hz timer, demo tasks and the shared shell; `--net` adds the polled virtio-net-device MMIO NIC on SLIRP (needs `-global virtio-mmio.force-legacy=false`, which run.sh sets) |
| NTFS (optional, real image) | boot with a real Windows volume as the **first AHCI disk**: take the `run.sh` QEMU line and replace `-drive file=build/test.img,...` with `-drive file=<win.img>,format=raw,if=none,id=td0 -device ide-hd,drive=td0,bus=sata.0` (the ESP drive stays), then `ls /mnt/win0` and `ls /mnt/win0/Windows/System32` | a read-only listing/read of a production NTFS volume; no Windows image ships with the repo, so this is a manual/CI-optional check (the fixture gate is `tools/smoke-ntfs.sh`) |
| GPU (real AMD, M12-6 follow-up) | build the `rescue` profile, boot on the machine with one AMD RX 500/6000 GPU, run `gpu` (and read the boot `gpu:` block): record the identity, BAR sizes, the `gpu: pcie link …` line and the thermal line | the real-hardware half of `M12_TOOLS_HW.md` §7; QEMU display models expose no PCIe capability and its DSDT has no thermal zone, so both branches are untested at runtime here and the serial transcript is attached to the follow-up issue |

Useful shell commands: `help`, `bootinfo` (both in the minimal kernel), and
with the `rescue` profile `lsos`, `diskhealth` (SMART needs AHCI/NVMe; the
BIOS IDE path degrades), `cat <path>`, `clone <src> <dst>`
(`CONFIG_IMAGER`) and `gpu` (the M12-6 read-only GPU/PCI report,
`CONFIG_GRAPHICS`). Quit QEMU with `Ctrl-A X`. For fixture variants
(`--broken`, `--keys`, `--shell-repair`, `--nvme`, `--smm`, `--imager`) see
the flags table above.

## 6. Driving and debugging a run

- **Serial console**: `-nographic` maps it to your terminal; `Ctrl-A X` quits.
- **QEMU monitor**: run with a monitor socket/channel when you need
  `sendkey`, `info registers`, or `xp` memory dumps (see `tools/kbd_test.sh`
  for a working example on x86).
- **Interrupt trace**: for riscv, `-d int -D build/qemu-int.log` records
  every trap with `cause/epc/tval` — invaluable for page-fault debugging.
- **Execution trace**: `-d int,exec` is large but shows the exact TB
  sequence; grep for the last `riscv_cpu_do_interrupt` before a hang.
- **Machine-check on riscv**: the kernel prints `trap:`/`exc` lines with
  `scause/stval/sepc`; `[user]` faults also name the killed task.

## 7. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| x86: no serial output | OVMF path not found (`--smm` needs `build/ovmf-smm/`) |
| x86: NVRAM repair refused | run with `--smm`; plain OVMF rejects SetVariable |
| Stack/heap corruption after a repair | the 4 KiB copy windows exist because kernel stacks are 16 KiB — keep stack buffers small |
| riscv: no virtio disk | pass `--disk`; legacy transports need `-global virtio-mmio.force-legacy=false` (run.sh sets it) |
| `Failed to get "write" lock` on `build/test.img` | another QEMU (e.g. a background smoke) holds the disk; wait or stop it |
| QEMU survives a smoke timeout | the scripts use `--signal=KILL`; a stray instance can be killed by matching its disk path |
| `cargo` cannot find `core` | wrong toolchain — use the script wrapper or `export PATH="$HOME/.cargo/bin:$PATH"` |
| aarch64 network phase SKIPs | the local `qemu-system-aarch64` was built without SLIRP; rebuild it with the `user` netdev backend (the smoke prints the reason) |
| aarch64: no `virtio-net up` | run QEMU with `-global virtio-mmio.force-legacy=false` (run.sh sets it); the driver binds the version-2 MMIO transport only |

## 8. Maintenance

- **Regenerate a fixture**: `python3 tools/mkdisk.py --<variant> build/test.img`.
- **SMM vars store**: phases 11–13 copy `build/ovmf-smm/OVMF_VARS_4M.fd`
  to `build/OVMF_VARS.smm.fd` first; delete that file to reset by hand.
- **Logs**: `build/smoke*.log` are the raw serial transcripts; keep the
  failing one for evidence when reporting a regression.
- **Line-size rule**: `find . -name '*.rs' ...` — nothing under `kernel*`,
  `abi`, `user`, `boot`, `drivers` may exceed 300 lines.

## 9. Release operations

The v0.0.4 release (M12) bumped the workspace to `0.0.4`, updated the
banners to `fantuan v0.0.4` / `fantuan-boot v0.0.4`, and added the disk
imager (`clone` + `--continue` bad-sector policy and report), read-only
NTFS (`/mnt/win0`), the read-only AMD/PCI GPU report (QEMU-only
acceptance) and virtualization V1 detection. Verification:
`tools/smoke-imager.sh` PASS, `tools/smoke-imager-bad.sh` PASS,
`tools/smoke-ntfs.sh` PASS, `tools/smoke-gpu.sh` PASS,
`tools/smoke-config.sh` PASS, `tools/smoke-bios.sh` 2/2, zero-warning
builds on x86_64 minimal/rescue/net/tls, riscv64, i686 and aarch64. The
v0.0.4 tag stays local and owner-gated (see `M12_TOOLS_HW.md`); this
repository does not create or push tags.

The v0.0.3 release (M11) bumped the workspace to `0.0.3`, updated the
banners to `fantuan v0.0.3` / `fantuan-boot v0.0.3`, added the aarch64
direct-FDT + virtio-net/TLS smoke phase and this matrix's aarch64 rows, and
re-ran the matrix on the release build: `tools/smoke-aarch64.sh` PASS (both
phases), `tools/smoke-net.sh` PASS (x86_64 offline gate),
`tools/smoke-config.sh` PASS, `tools/smoke-bios.sh` 2/2,
`tools/smoke-riscv.sh` 3/3, zero-warning builds on x86_64 minimal/net/tls,
riscv64, i686 and aarch64 minimal/net/tls. The v0.0.3 tag stays local and
owner-gated (see `M11_PLAN.md`); this repository does not create or push
tags.

The v0.0.2 release (M10) bumped the workspace to `0.0.2`, updated the
banners to `fantuan v0.0.2`, added this boot/support matrix and the
`WINDOWS.md` non-support page, and re-ran the full matrix on the release
build: `tools/smoke.sh` 13/13, `tools/smoke-bios.sh` 2/2,
`tools/smoke-riscv.sh` 3/3, `tools/smoke-iso.sh` 2/2, three release builds
with zero warnings. The v0.0.2 tag stays local and owner-gated (see
`M10_PLAN.md` §W6).

The v0.0.1 release followed the checklist in
`docs/M9_KERNEL_v0.0.1.md` §11: workspace bumps to `0.0.1`, banners print
`fantuan v0.0.1`, README/DESIGN/plan updated, the full matrix re-run on the
release build (`smoke.sh` 13/13, `smoke-riscv.sh` 3/3), then
`git tag -a v0.0.1`. **Tags and commits stay local until a push is
explicitly requested.** The audit that gated the release is
`docs/M9_AUDIT.md`.
