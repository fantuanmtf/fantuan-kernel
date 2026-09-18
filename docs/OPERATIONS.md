# Operations Guide

Day-2 operations for the fantuan-kernel test rigs: running the suites,
driving QEMU, reading logs, troubleshooting and releasing. For the shell
itself see [USAGE.md](USAGE.md); for builds see [BUILD.md](BUILD.md).

## 1. Automation at a glance

| Script | Purpose | Typical duration |
|---|---|---|
| `tools/build.sh [--arch riscv64]` | build all artifacts for one arch | < 1 min |
| `tools/smoke.sh` | full x86 acceptance suite (13 phases) | ~25 min |
| `tools/smoke-riscv.sh` | riscv acceptance suite (3 phases) | ~4 min |
| `tools/run.sh [flags]` | single interactive/scripted boot | until you quit |
| `tools/kbd_test.sh` | QEMU-monitor keyboard injection (x86) | ~1 min |

Both smoke scripts bound every QEMU run with `timeout --signal=KILL` and
exit non-zero on the first failing phase. Logs land in `build/*.log`.

## 2. The x86 suite (`tools/smoke.sh`, 13 phases)

| # | Phase (PASS string suffix) | What it proves |
|---|---|---|
| 1 | boot path is read-only: no repair writes | normal boot + negative write greps |
| 2 | broken-ESP repair via shell YES confirmation | consent-gated fallback copy |
| 3 | ext4 root, real fstab, /boot inventory, systemd/UKI | read-only root stack |
| 4 | no -smbios overrides: firmware defaults parsed | SMBIOS fallback |
| 5 | grub-fix install — backup, regenerate, publish, verify | config regeneration |
| 6 | crafted FAT: oversized file entry truncated | hostile-input robustness |
| 7 | 4 KiB clusters: short-data cluster write zero-pads | FAT write correctness |
| 8 | shell: autorun commands + surface scan + crypto KATs + idle notice | interactive shell |
| 9 | shell: confirmation-gated repair + cat of the repaired fallback | repair round-trip |
| 10 | NVMe: controller + VFS + SMART + boot repair | second block transport |
| 11 | Secure Boot: platform key enrolled | Setup Mode enrollment |
| 12 | Secure Boot: .auth verified, firmware store rejects authenticated variables | `.auth` structure/self-verify |
| 13 | NVRAM repair: delete/create/keep | three-boot SMM SetVariable sequence |

Phases 2, 9, 11–13 need the SMM OVMF build in `build/ovmf-smm/`
(`OVMF_CODE_4M.ms.fd` + `OVMF_VARS_4M.fd`); the script copies the vars
template before phases 11–13 because NVRAM repairs mutate it.

## 3. The riscv suite (`tools/smoke-riscv.sh`, 3 phases)

| Phase | What it proves |
|---|---|
| A (read-only) | boot, Sv39, FDT report, SBI timer, kernel + user tasks, fault kill/reap, virtio-blk, FAT32/ext4/probe, boot-repair diagnosis, UART shell (`diskhealth`, `cat`), and **no repair writes** |
| B (repair YES) | `grub-fix repair` + `YES` over virtio-blk: FIXED.TXT write+readback, fallback shim copy verified |
| C (repair NO) | the confirmation gate aborts with nothing written |

Phase A drives the shell by piping commands into QEMU's serial console
(`sleep 5; printf 'diskhealth\ncat HELLO.TXT\n'; sleep ...`) — the pipe
stays open so QEMU does not see EOF. The same technique drives phases B/C.

## 4. `tools/run.sh` flags

| Flag | Effect |
|---|---|
| `--arch x86_64\|riscv64` | select the platform (default x86_64) |
| `--graphics` | x86: show the GOP window instead of `-nographic` |
| `--broken` / `--broken-shim` | build the ESP without fallback / without shim |
| `--two-fs` | add the ext4 root + XFS probe fixture to the disk |
| `--keys` | Secure Boot certs + autorun shell script |
| `--shell-repair` | autorun runs `grub-fix repair` and answers YES |
| `--grub-regen` | autorun runs `grub-fix install` (implies `--two-fs`) |
| `--smm` | x86 on q35 with SMM OVMF (required for SetVariable) |
| `--no-smbios` | boot without `-smbios` overrides (firmware defaults) |
| `--nvme` | attach the disk as NVMe instead of AHCI |
| `--bigcluster` | 4 KiB-cluster FAT fixture |
| `--liar` | crafted FAT with an oversized file entry |
| `--disk` (riscv) | attach `build/test.img` as virtio-blk |
| `--keys`/`--shell-repair` (riscv) | forward the autorun fixture to mkdisk |

x86 `run.sh` also rebuilds the ESP in `build/esp/` and objcopies the kernel
to `build/esp/fantuan/kernel.bin` on every run.

## 5. Driving and debugging a run

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

## 6. Troubleshooting

| Symptom | Cause / fix |
|---|---|
| x86: no serial output | OVMF path not found (`--smm` needs `build/ovmf-smm/`) |
| x86: NVRAM repair refused | run with `--smm`; plain OVMF rejects SetVariable |
| Stack/heap corruption after a repair | the 4 KiB copy windows exist because kernel stacks are 16 KiB — keep stack buffers small |
| riscv: no virtio disk | pass `--disk`; legacy transports need `-global virtio-mmio.force-legacy=false` (run.sh sets it) |
| `Failed to get "write" lock` on `build/test.img` | another QEMU (e.g. a background smoke) holds the disk; wait or stop it |
| QEMU survives a smoke timeout | the scripts use `--signal=KILL`; a stray instance can be killed by matching its disk path |
| `cargo` cannot find `core` | wrong toolchain — use the script wrapper or `export PATH="$HOME/.cargo/bin:$PATH"` |

## 7. Maintenance

- **Regenerate a fixture**: `python3 tools/mkdisk.py --<variant> build/test.img`.
- **SMM vars store**: phases 11–13 copy `build/ovmf-smm/OVMF_VARS_4M.fd`
  to `build/OVMF_VARS.smm.fd` first; delete that file to reset by hand.
- **Logs**: `build/smoke*.log` are the raw serial transcripts; keep the
  failing one for evidence when reporting a regression.
- **Line-size rule**: `find . -name '*.rs' ...` — nothing under `kernel*`,
  `abi`, `user`, `boot`, `drivers` may exceed 300 lines.

## 8. Release operations

The v0.0.1 release followed the checklist in
`docs/M9_KERNEL_v0.0.1.md` §11: workspace bumps to `0.0.1`, banners print
`fantuan v0.0.1`, README/DESIGN/plan updated, the full matrix re-run on the
release build (`smoke.sh` 13/13, `smoke-riscv.sh` 3/3), then
`git tag -a v0.0.1`. **Tags and commits stay local until a push is
explicitly requested.** The audit that gated the release is
`docs/M9_AUDIT.md`.
