# Usage Guide (Rescue Operator)

What fantuan-kernel does, how to boot it, and how to use the shell to
diagnose and repair a broken machine. For building it see
[BUILD.md](BUILD.md); for the smoke suites and flag reference see
[OPERATIONS.md](OPERATIONS.md).

## 1. What this is

A self-written rescue kernel. It boots on bare metal or in QEMU, inspects
storage and boot-chain problems read-only, and only writes when an operator
explicitly turns on repair mode and confirms with `YES`. Two architectures:

| | x86_64 | riscv64 |
|---|---|---|
| Boot path | self-written UEFI bootloader (GOP + serial) | OpenSBI (`-bios default`) |
| Console | 16550 serial + GOP framebuffer mirror | NS16550 MMIO UART |
| Storage | AHCI (reference) and NVMe | virtio-mmio block |
| UEFI/NVRAM repair | yes (Runtime Services) | no (diagnosis only, honest degrade) |
| Shell commands | 12 | 9 (no hwdiag/lsdev/crypto-selftest) |

Current release: **v0.0.1** (see [HANDOVER.md](HANDOVER.md) for status).

## 2. Quick start

```sh
tools/run.sh                                  # x86, headless serial console
tools/run.sh --graphics                       # x86, GOP window + serial
tools/run.sh --smm --shell-repair             # x86, NVRAM writes need SMM OVMF
tools/run.sh --arch riscv64 --disk --two-fs   # riscv + virtio-blk + FAT/ext4
```

Quit QEMU with `Ctrl-A X` (headless) or close the window.

## 3. What a boot looks like

x86: bootloader banner -> `handshake ok` -> memory map summary ->
hardware/storage diagnostic stages -> VFS mount -> userland demo tasks ->
the shell prompt `shell> `.

riscv: OpenSBI banner -> `fantuan v0.0.1 (riscv64)` -> FDT memory/CPU
report -> Sv39 tables -> `blk: virtio registered` -> VFS + read-only
boot-repair diagnosis -> scheduler and user tasks -> `shell> `.

The shell is also fed by an autorun script when the ESP contains
`EFI/fantuan/shell.cmd` (see the `--keys` / `--shell-repair` fixtures).

## 4. Shell commands

| Command | What it does |
|---|---|
| `help` | lists the command table |
| `hwdiag` | re-runs hardware diagnostics (x86 only) |
| `lsdev` | lists PCI storage/display devices + drive identity (x86 only) |
| `lsos` | filesystems per partition (the probe table) |
| `lsmnt` | current mount aliases |
| `mount esp0 /mnt/esp0` | read-only alias for the ESP |
| `umount <path>` | removes an alias |
| `cat <path>` | prints a file (FAT or ext4, up to 4 KiB) |
| `bootinfo` | boot handover details (memory map, framebuffer, RSDP, ...) |
| `diskhealth [--scan]` | identity + SMART; `--scan` reads the surface (`q` cancels) |
| `grub-fix [diagnose\|repair\|install]` | boot-repair chain (see below) |
| `crypto-selftest` | SHA-256/RSA known-answer tests (x86 only) |

Examples:

```
shell> lsos
shell> mount esp0 /mnt/esp0
shell> cat /mnt/esp0/EFI/fantuan/shell.cmd
shell> diskhealth
shell> grub-fix                 # same as diagnose: read-only report
```

On virtio storage `diskhealth` prints
`SMART unsupported for this transport (virtio)` — no zero-filled fake
values.

## 5. The repair model (safety)

- **Read-only by default.** Every diagnostic, listing, mount and `cat` is
  read-only. The kernel enforces this with a `RepairToken`: write functions
  cannot even be called without it.
- **Two-step consent.** Writes require `grub-fix repair` (or `install`) and
  then typing `YES` at the `confirm>` prompt. Anything else aborts.
- **What repair does** (x86):
  - copies `EFI/ubuntu/shimx64.efi` to `EFI/BOOT/BOOTX64.EFI` when the
    fallback loader is missing;
  - writes and reads back `FIXED.TXT` as a self-test;
  - rebuilds the UEFI `BootOrder`, deletes stale entries and creates a
    missing ESP boot entry via `SetVariable` (needs SMM OVMF in QEMU);
  - `grub-fix install` backs up `grub.cfg`, regenerates it from the ext4
    root (`/boot` inventory + `/etc/fstab`), publishes and re-reads it.
- **On riscv** the FAT write path and fallback copy work over virtio-blk;
  the NVRAM half reports `runtime services unavailable` and is skipped.

## 6. Typical rescue workflows

1. **Diagnose a no-boot machine** — boot the kernel, read the bootrepair
   report: ESP scan, `grub.cfg` search UUID, `/etc/fstab` cross-check,
   `/boot` inventory, recommendations.
2. **Inspect files without mounting on the host** — `mount esp0 /mnt/esp0`,
   then `cat` configs; read the ext4 root read-only with `cat` too.
3. **Repair a missing fallback loader** — `grub-fix repair`, answer `YES`;
   watch for `repair: copied ... (N bytes, verified)`.
4. **Restore a boot entry** — same command: stale entries are deleted, a
   missing ESP entry is created and verified.
5. **Regenerate the GRUB configuration** — `grub-fix install`, answer
   `YES`; a `GRUBCFG.BAK` is kept.
6. **Check disk health** — `diskhealth`, and `diskhealth --scan` for a
   bounded (4 GiB cap) surface scan with live progress.

## 7. RISC-V notes

- Storage is virtio-mmio; attach a disk with `--disk` (the script passes
  `-global virtio-mmio.force-legacy=false` for the modern transport).
- There are no UEFI Runtime Services, so NVRAM/Secure Boot features are
  absent; `grub-fix diagnose` still checks the ESP, `grub.cfg`, `fstab`
  and the ext4 root.
- `diskhealth` reports SMART as unsupported for virtio (honest).
- Userland tasks run in U-mode with per-task Sv39 page tables; a fault in
  user mode kills and reaps the task, never the kernel.
