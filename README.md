# fantuan-kernel

A self-written operating system kernel targeting **live rescue systems**:
diagnose hardware and boot-chain problems, then repair them — Linux first,
BSD diagnosis, Windows deferred.

All repository artifacts are in English. The authoritative design lives in
[docs/DESIGN.md](docs/DESIGN.md) — change it before changing code.

## Current state: M7.6 + M7.7 + M7.8 + M5.5 + M8 (storage)

M0-M6 are in: UEFI boot chain, interrupts, higher-half kernel + frame
allocator, scheduler + syscall ABI, ring-3 user mode with an ELF loader,
C/Rust driver boundary (AHCI), the read-only diagnostics framework, and the
VFS (GPT/MBR + FAT32 + the ext4 read-only root driver, M6.5). M7 added
boot-repair diagnosis v1 (ESP scan, grub.cfg
+ fstab parsing, UUID/PARTUUID cross-checks); M7.5a added NVRAM diagnosis
via UEFI Runtime Services; M7.5b added FAT32 repair writes behind an
explicit repair-mode gate plus the fallback-loader repair; M7.6 adds NVRAM
repair via SetVariable — BootOrder rebuild, stale-entry deletion, and
explicit ESP boot-entry creation (device paths built from the partition
table). Runtime NVRAM writes are tested under QEMU q35 + SMM OVMF
(`tools/run.sh --smm`). M5.5 (finished after M7.6) adds the SMBIOS parser
(config-table + F-segment discovery), SMART disk health (power-on hours,
reallocated/pending/uncorrectable, ATA SSD detection), filesystem type
probing with the probe-only contract for non-FAT, the PCI device catalog,
and the driver handle API (`blk_open`/`blk_identify`/SMART ops).
M7.7 adds Secure Boot key inventory and the Setup-Mode platform-key
enrollment path; M7.8 (§10) adds the built-in minimal shell — serial line
editor with the 11 rescue commands, ESP autorun script, surface scan and
confirmation-gated repair. M8 (storage part) puts storage behind a
driver-agnostic ops registry: the new NVMe driver registers the same table
as AHCI, so the VFS, boot repair and SMART run unchanged over either
transport (`tools/run.sh --nvme`); 64-bit NVMe BARs above 4 GiB are mapped
on demand by `mm::paging::map_mmio`. A three-round audit (P0-P2) then closed
the remaining violations of the read-only iron rule and the latent
robustness bugs it found (FAT/ELF/bootloader input validation, hardware
paths, the `RepairToken` write capability). M7.9 completes the repair chain:
/boot inventory and systemd-boot/rEFInd/UKI diagnosis, a deterministic
grub.cfg generator, and `grub-fix install` (backup, regenerate, publish,
verify, NVRAM entry) per the §9.1 write contract. tools/mkdisk.py hand-builds
the GPT + FAT32 test disk including the ESP fixture (`--broken` /
`--broken-shim` / `--two-fs` / `--keys` / `--shell-repair` / `--liar` /
`--bigcluster` / `--grub-regen` variants).

- `boot/` — self-written UEFI bootloader in Rust (`x86_64-unknown-uefi`),
  hand-rolled against the UEFI spec: GOP, RSDP, memory map, kernel load via
  Simple File System Protocol, ExitBootServices with map-key retry.
- `boot/entry.S` — the "forever assembly": kernel entry stub (GDT, stack,
  BSS zeroing).
- `kernel/` — minimal `no_std` Rust kernel (`x86_64-unknown-none`):
  BootInfo handshake validation, 16550 serial, PIT sleep + PC-speaker beeps,
  GOP framebuffer console with the embedded Spleen 8x16 font.
- `abi/` — the versioned BootInfo ABI shared by both sides.

## Quickstart

Requirements: Rust (stable) with targets `x86_64-unknown-uefi` +
`x86_64-unknown-none`, QEMU, OVMF (`edk2-ovmf`), binutils (objcopy),
python3 (font regeneration only).

```sh
rustup target add x86_64-unknown-uefi x86_64-unknown-none
tools/run.sh              # headless: everything on the serial console
tools/run.sh --graphics   # with a window (framebuffer console + serial on stdio)
tools/smoke.sh            # CI-style: bounded run, greps for the handshake
```

In QEMU, quit with `Ctrl-A X` (headless) or close the window.
