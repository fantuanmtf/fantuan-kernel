# Handover Guide

Everything a new maintainer needs to take over fantuan-kernel: what it is,
where it stands, how it is put together, what is fragile, and what to do
first. Read this, then [DESIGN.md](DESIGN.md) (authoritative), then the
role guides: [BUILD.md](BUILD.md), [USAGE.md](USAGE.md),
[OPERATIONS.md](OPERATIONS.md), [DEVELOPMENT.md](DEVELOPMENT.md).

## 1. What this project is

A self-written operating system kernel for **live rescue systems**:
diagnose hardware and boot-chain problems from its own kernel, inspect
filesystems read-only, and repair boot loaders/NVRAM only after explicit
operator consent. Research vehicle as much as product: the same portable
core now runs on two architectures.

- **Primary target**: x86_64 UEFI rescue kernel (the product).
- **Second target**: riscv64 under OpenSBI on QEMU `virt` (portability
  proof of the arch split, not a hardware-support claim).
- **Out of scope for v0.0.1**: SMP, networking, USB, graphics beyond the
  GOP console, real RISC-V boards, authenticated Secure Boot on riscv.

## 2. Release status

- **Version**: v0.0.1, tag `v0.0.1` (annotated, **local only** — nothing
  has been pushed).
- **Verified at release**: `tools/smoke.sh` 13/13 PASS (incl. SMM NVRAM
  repair), `tools/smoke-riscv.sh` 3/3 PASS (read-only, repair YES, repair
  NO), zero warnings on both targets, no source file > 300 lines.
- **Audit**: `docs/M9_AUDIT.md` (18 checks with evidence; all release
  blockers fixed or recorded).
- **Milestone plan**: `docs/M9_KERNEL_v0.0.1.md` (M9.0–M9.5 closed).

## 3. Architecture in one page

```
                    abi (BootInfo, PHYS_OFFSET, syscall numbers)
                   /    \
      boot/ (UEFI)        OpenSBI
          |                  |
   kernel/ (x86_64)     kernel-riscv/ (riscv64)
          \                /
           kernel-core (portable half)
     frame | task | syscall | elf | user | log | input | time | mem
     drv   | vfs  | diag    | bootrepair | shell | runtime(UEFI FFI)
                        |
                 drivers/c (blk_ops)
              ahci | nvme | i8042 | virtio_mmio
```

- **Boot (x86)**: bootloader loads GOP/RSDP/memmap, `ExitBootServices`,
  passes `BootInfo`; kernel maps the higher half and starts.
- **Boot (riscv)**: OpenSBI enters S-mode with `a0=hartid, a1=DTB`; the
  kernel parses the FDT, synthesizes a BootInfo, builds Sv39 tables
  (identity + `PHYS_OFFSET` alias) and enters the high half.
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

## 5. Milestone history (where the bodies are buried)

| Milestone | What landed | Docs |
|---|---|---|
| M0–M2 | UEFI bootloader, BootInfo, serial, higher half + frame allocator | DESIGN §3–§4 |
| M3–M4 | scheduler, syscall ABI, ring-3 user mode + ELF loader | DESIGN §4.6 |
| M5 | diagnostics, SMART, PCI, probe table | DESIGN §6–§7 |
| M6 | VFS: GPT/MBR, FAT32, ext4 ro | DESIGN §8 |
| M7 | boot-repair chain: diagnosis, FAT writes, NVRAM, Secure Boot, shell | DESIGN §9–§10 |
| M8 | hardening (W^X/SMEP/SMAP), crypto KATs, PS/2 + GOP mirror, NVMe, authenticated variables | DESIGN §11–§13 |
| M9 | kernel-core extraction, riscv port (boot/Sv39/traps/U-mode), virtio-mmio, shared rescue stack, audit | `M9_KERNEL_v0.0.1.md`, `M9_AUDIT.md` |

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

## 7. Roadmap

1. **M10 — chroot compatibility (design first)**: a linuxulator-style
   Linux ELF + syscall translation layer and a BSD-ABI path so the rescue
   system can chroot into the target and run its own tools. The first
   deliverable is a design document (`docs/M10_CHROOT.md`): scope, ABI
   risks, ELF/syscall surface, staging. No code before it.
2. **Post-v0.0.1**: documented in `docs/M9_KERNEL_v0.0.1.md` §12 —
   authenticated Secure Boot on riscv, real-board support, SMP,
   networking, USB, richer graphics.

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
  [OPERATIONS.md](OPERATIONS.md) §4.
- **How do I reproduce a field bug?** Capture the full serial log;
  bootrepair prints every cross-check with the values it compared, and
  `run.sh` logs are complete transcripts.
- **Why no unit-test suite?** Bare-metal binaries; the smoke phases are
  the tests. Keep them exact-string based and update them with any output
  change.
