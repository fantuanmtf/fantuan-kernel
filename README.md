# fantuan-kernel

A self-written kernel for **Live environments** (RAM-first, clean
shutdown, optional persistence): diagnose hardware and boot-chain
problems, then repair them — Linux and BSD only; Windows repair is
permanently out of scope ([docs/WINDOWS.md](docs/WINDOWS.md), use WinPE).

**Direction (2026-09, declared).** The project transitions from a
rescue-only system to the kernel of a Live OS. The default build is the
minimal kernel plus the shell (boot + kernel + shell <= 300 MiB); tools,
networking, TLS, graphics, virtualization and the desktop are opt-in
through a menuconfig-style configuration ([docs/CONFIG_PLAN.md](docs/CONFIG_PLAN.md)),
and a content-hashed config keeps rebuilds incremental. Userland tools
are add-ons, never linked into the kernel or base, so no GPL code can
infect them; the desktop scope is XFCE and CDE only. The Live profile
wipes RAM on clean shutdown (`CONFIG_SECURE_WIPE`) as a best-effort
cold-boot (RAM-freezing) mitigation, with the limitations documented.
Tools and packages are developed on the `fantuan-apps` and `package`
branches and vendored into `apps/` per configuration
([docs/APPS.md](docs/APPS.md)).

All repository artifacts are in English. The authoritative design lives in
[docs/DESIGN.md](docs/DESIGN.md) — change it before changing code.

## Documentation

| Guide | Audience |
|---|---|
| [docs/USAGE.md](docs/USAGE.md) | Live operators: booting, the shell, profiles, rescue workflows, support matrix |
| [docs/APPS.md](docs/APPS.md) | app/package authors: branch model, vendoring, manifests, licensing |
| [docs/BUILD.md](docs/BUILD.md) | building from source: toolchains, all three arches, disk fixtures |
| [docs/OPERATIONS.md](docs/OPERATIONS.md) | the smoke suites, the boot/test matrix, `run.sh` flags, troubleshooting |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | conventions, extension recipes, testing, debugging |
| [docs/HANDOVER.md](docs/HANDOVER.md) | taking over: status, architecture, known debt, roadmap |
| [docs/DESIGN.md](docs/DESIGN.md) | the design of record (read before code changes) |
| [docs/M10_PLAN.md](docs/M10_PLAN.md) | the M10 (v0.0.2) completion plan and release checkpoints |
| [docs/M10_BOOT_32BIT.md](docs/M10_BOOT_32BIT.md) | design: BIOS boot chain + i686 port (v0.0.2) |
| [docs/WINDOWS.md](docs/WINDOWS.md) | Windows/PE non-support: why, and what to use instead |
| [docs/M9_KERNEL_v0.0.1.md](docs/M9_KERNEL_v0.0.1.md) | the M9/v0.0.1 milestone plan and release checklist |
| [docs/M9_AUDIT.md](docs/M9_AUDIT.md) | the v0.0.1 technical-debt and vulnerability audit |
| [docs/ROADMAP_v0.0.2+.md](docs/ROADMAP_v0.0.2+.md) | the v0.0.2 -> v1.0.0 roadmap (M10–M16) |
| [docs/M14_LINUXUSERS.md](docs/M14_LINUXUSERS.md) | design: Linux userspace, bootstrap chain, hypervisor V2 (v0.1.0) |
| [docs/M11_NET.md](docs/M11_NET.md) | design: NetBSD-derived network stack (v0.0.3) |
| [docs/M12_TOOLS_HW.md](docs/M12_TOOLS_HW.md) | design: disk imager, NTFS read-only, GPU probe, virt detection (v0.0.4) |
| [docs/M13_GRAPHICS.md](docs/M13_GRAPHICS.md) | design: framebuffer/input, KMS-like + repair-IPC contracts (v0.0.5) |
| [THIRD_PARTY.md](THIRD_PARTY.md) | third-party components, licenses and origin register |
| [docs/PROGRESS.md](docs/PROGRESS.md) | live progress tracker for v0.0.2 -> v0.1.5 |

## Current state: v0.0.2 — M10 complete (legacy BIOS + i686)

M10 makes the Live kernel reachable on machines without UEFI and on
32-bit x86 CPUs. A self-written BIOS chain (`boot-bios/` stage1 in the MBR
plus stage2) collects E820, sets a VBE mode when available, loads the flat
kernel and hands over a `BootInfo arch=3`. The x86_64 kernel boots from it
with a serial-only console and the full shell; the new `kernel-i686/` port
brings up 32-bit PSE paging, a 3G/1G memory split capped at 1 GiB, frame
allocator, IDT/PIC/PIT, round-robin scheduling, ring 3 with ELF32 user
tasks over `int 0x80`, a VBE text console with serial fallback, and a
read-only PIO ATA + shared-VFS path to the test disk. M10 also ships
`tools/mkiso.py` (a self-written ISO9660 + El Torito builder) for a hybrid
BIOS+UEFI ISO under the 1 GiB budget. This is the v0.0.2 release; the full
verification is `smoke.sh` 13/13, `smoke-bios.sh` 2/2,
`smoke-riscv.sh` 3/3, `smoke-iso.sh` 2/2 (see
[PROGRESS.md](docs/PROGRESS.md) and [OPERATIONS.md](docs/OPERATIONS.md)
§3). Windows boot repair stays permanently unsupported
([WINDOWS.md](docs/WINDOWS.md)).

M9 brings up a second architecture and splits the portable half of the
kernel into `kernel-core`: the riscv64 kernel boots under OpenSBI (QEMU
`virt`), builds Sv39 tables with an identity + PHYS_OFFSET alias, runs a
trap frame + SBI timer + the shared scheduler, runs user tasks in U-mode
with per-task roots (ELF loader from `kernel-core`, `ecall` syscalls,
fault kill + reap), and speaks modern virtio-mmio for block storage. The
shared VFS, disk probe table, read-only boot-repair diagnosis and the
interactive shell all run on RISC-V; SMART is reported explicitly as
unsupported for virtio, and UEFI/NVRAM paths degrade honestly (no runtime
services there). x86 remains the product and keeps the full 13-phase smoke
suite green through re-exports; see `docs/M9_KERNEL_v0.0.1.md` and the
audit in `docs/M9_AUDIT.md`.

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
- `boot-bios/` — self-written legacy-BIOS chain: 512-byte stage1 (MBR,
  `int 0x13` LBA) + stage2 (E820, VBE, long mode / 32-bit PIE paging,
  `BootInfo arch=3`).
- `boot/entry.S` — the "forever assembly": kernel entry stub (GDT, stack,
  BSS zeroing).
- `kernel/` — minimal `no_std` Rust kernel (`x86_64-unknown-none`):
  BootInfo handshake validation, 16550 serial, PIT sleep + PC-speaker beeps,
  GOP framebuffer console with the embedded Spleen 8x16 font.
- `kernel-i686/` — the 32-bit kernel (nightly + `targets/i686-fantuan-none.json`):
  32-bit paging, IDT/PIC/PIT, scheduler, ring 3/ELF32, PIO ATA, VBE console.
- `kernel-core/` — the portable half shared by all three kernels: frame
  allocator, scheduler, syscall semantics, ELF loader, VFS, diagnostics,
  boot repair and the shell, with per-arch hooks installed at boot.
- `kernel-riscv/` — riscv64 kernel (`riscv64gc-unknown-none-elf`): Sv39
  paging, traps, SBI timer, U-mode tasks and the virtio-mmio block driver
  glue (`drivers/c/virtio_mmio.c`).
- `abi/` — the versioned BootInfo ABI shared by both sides.

## Quickstart

Requirements: Rust (stable) with targets `x86_64-unknown-uefi`,
`x86_64-unknown-none` and `riscv64gc-unknown-none-elf`, nightly + `rust-src`
(i686 only), QEMU (x86_64 and riscv64), OVMF (`edk2-ovmf`), NASM (BIOS
stages), binutils (objcopy), clang + llvm-ar (riscv C drivers), python3
(disk/ISO fixtures).

```sh
rustup target add x86_64-unknown-uefi x86_64-unknown-none riscv64gc-unknown-none-elf
rustup toolchain install nightly --profile minimal --component rust-src
tools/run.sh                             # x86: serial console
tools/run.sh --graphics                  # x86: window (GOP console)
tools/run-bios.sh                        # x86_64: legacy BIOS chain
tools/run-bios.sh --arch i686            # i686: BIOS bring-up (no shell yet)
tools/run.sh --arch riscv64 --disk --two-fs   # riscv: OpenSBI + virtio-blk + shell
tools/smoke.sh                           # x86 CI-style (13 phases)
tools/smoke-bios.sh                      # BIOS CI-style (x86_64 + i686, 2 phases)
tools/smoke-riscv.sh                     # riscv CI-style (bounded, serial-fed shell)
tools/smoke-iso.sh                       # hybrid ISO CI-style (BIOS + UEFI, 2 phases)
```

In QEMU, quit with `Ctrl-A X` (headless) or close the window.
