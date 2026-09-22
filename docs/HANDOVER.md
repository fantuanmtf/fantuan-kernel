# Handover Guide

Everything a new maintainer needs to take over fantuan-kernel: what it is,
where it stands, how it is put together, what is fragile, and what to do
first. Read this, then [DESIGN.md](DESIGN.md) (authoritative), then the
role guides: [BUILD.md](BUILD.md), [USAGE.md](USAGE.md),
[OPERATIONS.md](OPERATIONS.md), [DEVELOPMENT.md](DEVELOPMENT.md).

## 1. What this project is

The kernel of a **Live OS** (RAM-first, clean shutdown, optional
persistence), self-written: the default minimal build is kernel + boot +
shell, and the opt-in `rescue` profile diagnoses hardware and boot-chain
problems from its own kernel, inspects filesystems read-only and repairs boot
loaders/NVRAM only after explicit operator consent. Research vehicle as much
as product: the same portable core now runs on two architectures.

- **Primary target**: x86_64 UEFI Live kernel (the product).
- **Second target**: riscv64 under OpenSBI on QEMU `virt` (portability
  proof of the arch split, not a hardware-support claim).
- **v0.0.4 adds**: the verified disk imager (`clone` + `--continue`
  bad-sector policy and report), read-only NTFS (`/mnt/win0`), the
  read-only AMD/PCI GPU report (QEMU-validated) and virtualization V1
  detection.
- **Previous releases**: v0.0.3 = the NetBSD-derived IPv4/TCP stack (DHCP,
  DNS, ICMP, HTTP) on x86_64 (e1000) and aarch64 (polled virtio-net MMIO on
  QEMU `virt`), pinned-CA HTTPS through mbedTLS, the aarch64 direct-FDT
  boot; v0.0.2 = self-written legacy-BIOS chain + i686 (paging, scheduler,
  ring 3/ELF32, read-only PIO ATA VFS, VBE console) + hybrid ISO; v0.0.1 =
  x86_64 UEFI + riscv64.
- **Out of v0.0.4 scope**: SMP, USB, PAE / >1 GiB on i686, the framebuffer/
  KMS graphics API (M13), real RISC-V/aarch64 boards, aarch64 UEFI (AAVMF;
  deferred to M14 with the loader port).

## 2. Release status

- **Version**: v0.0.4 (workspace + banners). The annotated `v0.0.4` tag is
  prepared for the owner and stays **local only** — nothing has been
  pushed; this repository does not create tags.
- **Verified at M12**: `tools/smoke-imager.sh` PASS,
  `tools/smoke-imager-bad.sh` PASS, `tools/smoke-ntfs.sh` PASS,
  `tools/smoke-gpu.sh` PASS (QEMU-only acceptance), `tools/smoke-config.sh`
  PASS, `tools/smoke-bios.sh` 2/2; zero warnings on the x86_64
  minimal/rescue/net/tls, riscv64, i686 and aarch64 builds; no source file >
  300 lines. The full matrix is in `docs/PROGRESS.md` snapshots.
- **Audit**: `docs/M9_AUDIT.md` (v0.0.1, 18 checks); open items are
  tracked under "Known issues" in `docs/PROGRESS.md`.
- **Milestone plans**: `docs/M12_TOOLS_HW.md` (M12 closed; next M13 V-a)
  and `docs/M11_NET.md` (M11 design of record); the v0.0.1 plan is
  `docs/M9_KERNEL_v0.0.1.md`; M10 is `docs/M10_PLAN.md`.
- **Current direction (C5, 2026-09)**: the default `minimal` profile is the
  kernel + boot + shell only; the diagnostic commands and boot repair live
  behind the `rescue` profile, the network tools are the non-default interim
  bridge whose catalog home is `apps/{ping,nslookup,wget}`, and bash's early
  port is recorded in `apps/bash/port/`. See `docs/CONFIG_PLAN.md` and
  `docs/APPS.md`.

## 3. Architecture in one page

```
                    abi (BootInfo, PHYS_OFFSET, syscall numbers)
                   /    \
      boot/ (UEFI)        OpenSBI        QEMU raw Image
          |                  |                |
   kernel/ (x86_64)    kernel-riscv/    kernel-aarch64/
          \                |               /
           \____________ kernel-core (portable half)
                        |
              kernel-net (opt-in, x86_64 + aarch64)
     frame | task | syscall | elf | user | log | input | time | mem
     drv   | vfs  | diag    | bootrepair | shell | runtime(UEFI FFI)
                        |
                 drivers/c (blk_ops)
              ahci | nvme | i8042 | virtio_mmio
```

- **Boot (x86)**: bootloader loads GOP/RSDP/memmap, `ExitBootServices`,
  passes `BootInfo`; kernel maps the higher half and starts.
- **Boot (x86 legacy BIOS)**: `boot-bios/stage1` (MBR) -> stage2 (E820,
  VBE mode set) -> long mode (x86_64) or 32-bit PSE paging (i686) ->
  `BootInfo arch=3`; no Runtime Services, so NVRAM repair degrades.
- **Boot (riscv)**: OpenSBI enters S-mode with `a0=hartid, a1=DTB`; the
  kernel parses the FDT, synthesizes a BootInfo, builds Sv39 tables
  (identity + `PHYS_OFFSET` alias) and enters the high half.
- **Boot (aarch64)**: QEMU's Linux-compatible raw-`Image` protocol enters
  EL1 with `x0=DTB`; the kernel parses the FDT, builds 4K-granule tables
  (TTBR0 identity + TTBR1 direct map), GICv2 + the EL1 timer at 100 Hz and
  runs the shared scheduler/shell. Network is the polled virtio-net MMIO
  device (no PCI on the QEMU `virt` machine).
- **Memory**: a bitmap frame allocator over the firmware/FDT map; no heap;
  `PHYS_OFFSET` is cfg-per-arch (`0xFFFF_8000...` / `0xFFFF_FFC0...`).
- **Tasks**: 16 slots, 16 KiB kernel stacks, round-robin at 100 Hz,
  sleep/reap; the portable scheduler drives arch switch hooks.
- **User mode**: ring 3 on x86 (per-task PML4), U-mode on riscv (per-task
  Sv39 root); syscalls `int 0x60` / `ecall`; ELF loader is shared.
- **Storage**: C `blk_ops` registry; the VFS (GPT/MBR, FAT32 rw-gated,
  ext4 ro, probe-only others) and all upper layers are transport-agnostic.
- **Repair model**: diagnostics read-only; writes need `RepairToken` +
  shell `YES`; x86 uses UEFI Runtime Services for NVRAM, riscv degrades.
- **Logging**: `kernel-core::log` sink installed per arch (16550/MMIO
  UART, x86 mirrors to GOP), LF->CRLF handled by the sink.

## 4. Key invariants (breaking these causes subtle bugs)

1. **riscv absolute-pointer rule**: the kernel links at `0x80200000` but
   runs via the alias; function pointers/jump tables hold link addresses.
   Kernel code always runs on the **kernel root**; per-task roots are
   entered only for U-mode and around the user-copy bridge.
2. **riscv non-leaf PTEs carry no U bit** (QEMU 10 rejects it); only
   leaves do.
3. **Repair gate**: no disk/NVRAM write outside `RepairToken` +
   `YES`; boot paths call `diagnose` only.
4. **ABI is append-only**: BootInfo appends + `BOOT_VERSION`, syscall
   numbers never renumber.
5. **No allocator**: fixed buffers, `static mut` scratch, 4 KiB copy
   windows — remember stacks are only 16 KiB.
6. **300-line files, English, docs-first, zero warnings** (see
   [DEVELOPMENT.md](DEVELOPMENT.md) §1).
7. **Multi-component licensing**: our code is BSD-3; bundled components keep
   their own licenses (NetBSD network stack, Spleen font, musl, tcc, and the
   developer-image XFCE/Qt as separate programs). `THIRD_PARTY.md` is the
   register — never import or bundle code without a declaration, and never
   link GPL/copyleft code into the kernel.

## 5. Milestone history (where the bodies are buried)

| Milestone | What landed | Docs |
|---|---|---|
| M0–M2 | UEFI bootloader, BootInfo, serial, higher half + frame allocator | DESIGN §3–§4 |
| M3–M4 | scheduler, syscall ABI, ring-3 user mode + ELF loader | DESIGN §4.6 |
| M5 | diagnostics, SMART, PCI, probe table | DESIGN §6–§7 |
| M6 | VFS: GPT/MBR, FAT32, ext4 ro | DESIGN §8 |
| M7 | boot-repair chain: diagnosis, FAT writes, NVRAM, Secure Boot, shell | DESIGN §9–§10 |
| M8 | hardening (W^X/SMEP/SMAP), crypto KATs, PS/2 + GOP mirror, NVMe, authenticated variables | DESIGN §11–§13 |
| M9 | kernel-core extraction, riscv port (boot/Sv39/traps/U-mode), virtio-mmio, shared VFS/diagnostic/repair stack, audit | `M9_KERNEL_v0.0.1.md`, `M9_AUDIT.md` |
| M10 | legacy BIOS boot chain (MBR/stage2/VBE), i686 port (32-bit paging, scheduler, ring 3/ELF32, read-only PIO ATA VFS), hybrid BIOS+UEFI ISO | `M10_BOOT_32BIT.md`, `M10_PLAN.md`, `PROGRESS.md` |

## 6. Known debt and risks

From `docs/M9_AUDIT.md` (all non-blocking for v0.0.1):

- Serial/hex formatting is duplicated between the two arch UART drivers;
  fold the helpers into `kernel-core::log`.
- `tools/run.sh` parses flags twice (prescan + main loop); a shared
  `env.sh`/parser would remove the duplication.
- riscv smoke gaps: NVRAM/SMM, Secure Boot, PS/2/GOP, SMBIOS,
  AHCI/NVMe, crypto-selftest, `diskhealth --scan`, hostile disk
  fixtures (repair writes are now covered).
- No explicit write-to-Rx W^X fault test on riscv.
- `probe::FsType::Ufs` and `part::Table.kind` are deliberately unused
  until a consumer exists.
- ASID is always 0 on riscv (global TLB flushes); fine for correctness,
  a future performance item.

M10 additions to the debt list:

- The intermittent riscv `uart::log_bytes` kernel fault
  (`scause=0xd stval=0x7f8 sepc=0x80200c4e`) still recurs occasionally in
  the repair phases; see `PROGRESS.md` "Known issues". Watch item for
  M11.
- i686 is deliberately read-only and has no interactive shell; the PIO ATA
  path is polling-only (no IRQ) and caps the direct map at 1 GiB.
- VBE mode setting is only exercised under QEMU/SeaBIOS; physical-firmware
  VBE differences are untested.
- The hybrid ISO is CD-ROM only (no isohybrid/USB `dd` support).

## 7. Roadmap

The full post-v0.0.2 plan is `docs/ROADMAP_v0.0.2+.md` (M11–M16):

1. **M10 — v0.0.2 (shipped)**: self-written BIOS boot chain + i686 port;
   design: `M10_BOOT_32BIT.md`, plan/verification: `M10_PLAN.md` and
   `PROGRESS.md`. Windows boot repair is permanently out of
   scope (WinPE recommended; see `WINDOWS.md`).
2. **M11 — v0.0.3 (shipped)**: ARM64 (QEMU virt) + `net_ops` + full TCP/HTTPS
   (NetBSD-derived stack + mbedTLS; no Linux net/ code — license;
   design: `M11_NET.md`).
3. **M12 — v0.0.4 (shipped)**: disk imager, NTFS read-only, AMD GPU probe,
   virtualization V1 detection.
4. **M13 — v0.0.5**: graphics/input API and the KMS-like + repair-IPC
   contracts.
5. **M14 — v0.1.0**: POSIX layer, musl port, in-system bootstrap
   (design: `M14_LINUXUSERS.md`), hypervisor V2.
6. **M15 — v0.1.5**: XFCE/Qt, disk-service VM (isolated mounting with a
   physical-mount fallback), the MinGW bootstrap script.
7. **M16 — v0.5.0/v1.0.0**: final compatibility matrix, audit, freeze.

## 8. First week checklist

1. Build both arches (`tools/build.sh`, `tools/build.sh --arch riscv64`).
2. Read this + DESIGN.md §2–§5 (conventions and architecture).
3. Boot x86 (`tools/run.sh`), then riscv (`tools/run.sh --arch riscv64
   --disk --two-fs`); try the shell commands in [USAGE.md](USAGE.md).
4. Run `tools/smoke-riscv.sh` (fast) and, when you have ~25 minutes,
   `tools/smoke.sh`.
5. Read `docs/M9_AUDIT.md` and pick a debt item from §6 above.
6. Make a trivial change end-to-end (e.g. a new shared shell command):
   code + both builds + smoke phase + commit with a `Verified:` paragraph.
7. Before releasing anything: ask before pushing or tagging.

## 9. FAQ / gotchas

- **Why is the riscv kernel linked low?** OpenSBI loads the payload at its
  ELF physical address (`0x80200000`); the kernel jumps to its alias after
  building Sv39 tables. See invariant 1.
- **Why do user tasks not need a kernel-root switch on x86?** The x86 user
  PML4 clones the kernel entries at the same link addresses, so kernel
  code is mapped identically; riscv's alias/link split is the special case.
- **Where do I add a flag for testing?** `tools/run.sh` (prescan loop for
  `--arch`/`--disk`-style flags, main loop for x86 fixtures) and
  `tools/mkdisk.py` for fixtures; document it in
  [OPERATIONS.md](OPERATIONS.md) §5.
- **How do I reproduce a field bug?** Capture the full serial log;
  bootrepair prints every cross-check with the values it compared, and
  `run.sh` logs are complete transcripts.
- **Why no unit-test suite?** Bare-metal binaries; the smoke phases are
  the tests. Keep them exact-string based and update them with any output
  change.
