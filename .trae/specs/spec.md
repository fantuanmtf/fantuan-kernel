# fantuan-kernel — Product Requirements Document (v1.0 Complete)

## Overview
- **Summary**: Complete the remaining development of fantuan-kernel per the official DESIGN.md, integrating all modules, running full tests, updating documentation, and managing version control through git/GitHub.
- **Purpose**: Deliver a complete, independently deployable fantuan-kernel OS rescue system (M0–M7.6 + deferred M5.5 features + Minimal Shell v1) that boots in QEMU/OVMF, diagnoses hardware and boot-chain issues, performs Linux boot repairs, and exposes a built-in command shell.
- **Target Users**: System administrators, rescue-media creators, and OS kernel developers.

## Goals
1. **Module Audit & Task Inventory**: Enumerate every DESIGN.md milestone, marking completed vs. pending modules with clear priorities and technical requirements.
2. **Remaining Feature Implementation**: Code, integrate, and locally verify every pending module strictly against DESIGN.md specifications.
3. **Full Integration & Testing**: Fix inter-module compatibility issues, run end-to-end functional/performance/compatibility tests, and produce a deployable build.
4. **Documentation Refresh**: Sync all technical docs (deployment guide, API reference, config reference, changelog/version history) with the final implementation.
5. **Git Hygiene & Remote Push**: Validate local commit conventions, batch-commit the full codebase + docs with reasonable granularity, and push to the specified GitHub remote with full history.

## Non-Goals
- No M8 (linuxulator) or M9 (RISC-V port) — explicitly future milestones.
- No NTFS, ext4, or UFS filesystem write paths beyond FAT32 repair writes.
- No Secure Boot key enrollment (M7.7 deferred).
- No new user-facing shell features outside §10 Minimal Shell v1 command set.
- No code-comments localization; repository artifacts remain English-only (DESIGN.md header policy).

## Background & Context
- Repository root: `/home/haiyan/fantuan-kernel`
- DESIGN.md lives at [docs/DESIGN.md](file:///home/haiyan/fantuan-kernel/docs/DESIGN.md) and defines milestones M0–M9 plus a §10 Minimal Shell v1 command table.
- Current README declares state M7.6; file tree under kernel/ covers bootrepair/, diag/, mm/, vfs/ plus the arch-level Rust core.
- drivers/c/ contains ahci.c only; DESIGN.md §5 requires NVMe as the second storage driver.
- DESIGN.md §6 defers M5.5 (SMBIOS, SMART, surface-scan, OS identification, auto ro-mount).
- DESIGN.md §10 defines a Minimal Shell v1 command set (hwdiag, lsdev, lsos, lsmnt, mount, umount, cat, help, bootinfo, diskhealth, grub-fix) — no shell.rs module currently exists in kernel/src/.
- Build toolchain: Rust stable with targets `x86_64-unknown-uefi` + `x86_64-unknown-none`, cc for C/asm, OVMF + QEMU for runs.
- Test disks are hand-built by `tools/mkdisk.py` (--broken / --broken-shim variants for bootrepair fixtures).

## Functional Requirements
- **FR-1 (Inventory)**: Produce an explicit per-milestone checklist of DESIGN.md items, each tagged `done` or `pending` with priority (high/medium/low) and a short technical spec.
- **FR-2 (M5.5 — SMBIOS Parser)**: Implement an SMBIOS entry-point scanner (32-bit `_SM_` and 64-bit `_SM3_` signatures from RSDP/EFI config tables) plus parsers for Type 0 (BIOS), Type 1 (System), Type 4 (Processor), Type 9 (System Slot — beep-code correlation per §6.2), Type 16 (Physical Memory Array), Type 17 (Memory Device). Wire Type-9 slot checks into the gpu diagnostic so the "2 short" beep code fires when a slot is marked In Use but no dGPU appears in PCI enumeration. Surface SMBIOS strings (sanitized to printable ASCII, DESIGN.md §3) in the stage-1 diagnostics report.
- **FR-3 (M5.5 — SMART / Disk Health, §7)**: Add ATA passthrough (IDENTIFY DEVICE data log page, SMART READ DATA / SMART READ LOG) over the existing AHCI blk_read path via the blk_ata_passthrough op (extend driver.h), extracting power-on hours (attr 0x09), reallocated (0x05), pending (0xC5), uncorrectable (0xC6), and standardized Device Statistics as fallback; for NVMe implement the Identify controller + Get Log Page (0x02 SMART / Health Information) once the NVMe driver lands. Display per-disk lines in decimal GB/TB matching DESIGN.md §7 example format.
- **FR-4 (M5.5 — Surface Scan)**: Add a diagnostic-level surface scan behind a runtime flag (§7: off by default, `diskhealth --scan` opt-in), reporting sectors >500 ms via TSC timing and any read errors, progress bar + cancellable (best-effort via polled early-exit).
- **FR-5 (M5.5 — OS Identification & auto ro-mount, §8)**: Implement superblock-magic filesystem probing on the first few KB of every partition (ext4 0xEF53, XFS "XFSB", Btrfs, FAT, NTFS "NTFS    ", swap, UFS), list them alongside bootloader identification (`\EFI\` scan + grub/vmlinuz/initramfs presence + GPT GUIDs). **Mount semantics in v1: only FAT32 supports actual data read-mounting; all other filesystems are type-probed only and never provide read/write data access**; an auto ro-mount attempt on a non-FAT detected root will degrade gracefully to a logged reason (e.g., "ext4: identified, not mounted (v1 read-only probe-only)") and produce no mount table entry.
- **FR-6 (NVMe Driver, §5)**: Add `drivers/c/nvme.c` implementing a polling read-only path (one admin queue + one I/O queue, PRP lists limited to single-page 4K transfers, Identify + Get Log Page for SMART per FR-3, NVM command set read). Extend Rust core PCI enumeration (pci.rs) to detect class 0x01 subclass 0x08 and hand the BAR0 to the C probe. Wire the storage device abstraction so VFS/partition scan prefers the first available AHCI or NVMe device.
- **FR-7 (Minimal Shell v1, §10)**: Implement a polled-line-editing shell module (kernel/src/shell.rs) on serial + GOP console input, exposing every command in the §10 table: `hwdiag`, `lsdev`, `lsos`, `lsmnt`, `mount <dev> <path>`, `umount <path>`, `cat <path>`, `help`, `bootinfo`, `diskhealth [-scan]`, `grub-fix [diagnose|repair]`. The shell enters after boot-complete (post-long-beep, replacing the current idle loop) and yields to the scheduler between keystrokes. `grub-fix repair` explicitly enables repair mode (vfs::enable_repair_mode) before any write.
- **FR-8 (Integration & Build Verification)**: `tools/build.sh` must succeed with zero warnings; `tools/smoke.sh` must report the handshake line and at least one shell prompt (or the boot-complete "beep: boot ok" line for headless). `tools/run.sh --smm` must complete an NVRAM repair self-test cycle (delete / create / keep) on the --broken fixture against a persistent QEMU vars file.
- **FR-9 (Documentation)**: Update [README.md](file:///home/haiyan/fantuan-kernel/README.md) with current state (new M5.5 + Shell status), add docs/API.md covering C driver ops and syscall ABI v1, add docs/DEPLOY.md covering image layout (ESP structure + kernel.bin location) + QEMU vs. real-machine instructions, add docs/CONFIG.md listing capability bits / kernel parameters / shell env; append docs/CHANGELOG.md with per-milestone version records from M0 through current final state.
- **FR-10 (Git Hygiene & Push)**: Ensure every local commit message follows the `area: short summary` Conventional Commits-lite format with no secrets; verify git status is clean after all changes; create a GitHub remote named `origin` (if absent) and push the default branch with `--follow-tags`; remote must contain full commit history and the final tree (code + build tooling + docs).

## Non-Functional Requirements
- **NFR-1 (Correctness)**: No kernel panic during smoke runs; every §4 handshake check (magic, version, PHYS_OFFSET) passes on every boot.
- **NFR-2 (Read-First Safety)**: In non-repair mode, no disk writes occur; repair mode is exclusively gated by vfs::enable_repair_mode() and `grub-fix repair`.
- **NFR-3 (Performance)**: Boot-to-shell on QEMU q35 (4 GiB RAM, 2 vCPUs) completes within 5 s wall-clock; surface-scan throughput ≥ 100 MiB/s on the test disk.
- **NFR-4 (Compatibility)**: Boots on plain OVMF (non-SMM), SMM OVMF (`--smm`), and headless serial-only QEMU (`-nographic`); works with the broken-ESP and broken-shim disk fixtures.
- **NFR-5 (Code Modularity)**: New source files comply with DESIGN.md §13.4a (≤ ~300 lines; module-map comment header; section banners).
- **NFR-6 (English-Only Artifacts)**: Code, comments, commit messages, and documentation follow DESIGN.md header language policy.

## Constraints
- **Technical**: no_std Rust kernel + freestanding C; large code model (-mcmodel=large); no libc; no heap allocator in v1 (static buffers only); physical pointers via PHYS_OFFSET alias; PCI config space stays in Rust core (C drivers never touch it).
- **Business**: Offline-first (v1 has zero network drivers); read-only-first (iron rule); Windows repair deferred.
- **Dependencies**: Rust stable + x86_64-unknown-uefi / x86_64-unknown-none targets; host cc for C/asm; QEMU system-x86_64 + edk2-ovmf; python3 for mkdisk/smoke/verify helpers.

## Assumptions
1. The bootloader / kernel BootInfo ABI (v1) stays append-only; new capability bits are acceptable but version field remains 1.
2. The shell does not require job control or readline; simple polled line editing with backspace is sufficient for v1.
3. Surface scan progress and cancellation work via a shared atomic flag (no preemption abort within a single sector read).
4. GitHub remote access is configured; if not, the agent will ask the user for the remote URL and any required authentication guidance.

## Acceptance Criteria

### AC-1: Module inventory is complete and accurate
- **Type**: `rule`
- **Given**: The DESIGN.md milestone table and the current source tree
- **When**: Inventory is produced in tasks.md with one task per pending module, each annotated with status/priority/scope
- **Then**: Every milestone M0–M7.6 is marked either covered-by-code or has a corresponding pending task; §10 Shell and M5.5 items are explicitly accounted for
- **Pass Condition**: tasks.md contains ≥ 4 pending implementation tasks covering SMBIOS, SMART+surface scan, OS identification, NVMe driver, and Shell; every DESIGN.md "Deferred to M5.5" bullet maps to at least one task
- **Evidence**: Grep of tasks.md headings against DESIGN.md §12 milestone names; diff of required vs. implemented modules

### AC-2: Build passes with zero errors and warnings
- **Type**: `rule`
- **Given**: A clean git tree with the final implementation applied
- **When**: `bash tools/build.sh` is executed from the repo root
- **Then**: cargo (user, kernel, boot) and cc compile and link with exit 0; no rustc or gcc warnings reach stderr
- **Pass Condition**: Exit code 0 and stderr contains no "warning:" lines (case-insensitive)
- **Evidence**: Captured build log; `wc -l` of warning-filtered output = 0

### AC-3: Smoke test boots cleanly and reaches idle/shell
- **Type**: `rule`
- **Given**: A freshly built image + test disk via tools/mkdisk.py
- **When**: `bash tools/smoke.sh` runs to completion
- **Then**: Serial log contains "handshake ok:", "diag: stage 1 summary:", "beep: boot ok", and either "shell:>" prompt or equivalent boot-complete marker
- **Pass Condition**: All four required log lines appear; no "PANIC:" substring
- **Evidence**: smoke.sh exit 0; grep against the captured serial log

### AC-4: SMBIOS diagnostic output is present
- **Type**: `rule`
- **Given**: A QEMU boot with `-smbios type=0,vendor=FOO` injected
- **When**: Stage-1 diagnostics run
- **Then**: The serial log's diag/cpu section reports at least one SMBIOS-derived string (BIOS vendor / version / system product name) sanitized to printable ASCII
- **Pass Condition**: Log contains "smbios:" and the injected vendor "FOO" substring
- **Evidence**: Boot log capture with explicit `-smbios` argument in the QEMU cmdline

### AC-5: Disk-health lines match DESIGN.md §7 format
- **Type**: `rule`
- **Given**: Boot with the standard test disk
- **When**: `diskhealth` shell command is issued
- **Then**: Each storage device line reports model + power-on hours + either SMART attr counts (HDD) or life % + read/written TB (NVMe) in decimal GB/TB units
- **Pass Condition**: At least one device line is emitted and matches regex pattern `\S+\s+(SSD|HDD)\s+power-on\s+\d+\s+h`
- **Evidence**: Shell command transcript from serial capture

### AC-6: OS identification lists the ESP filesystem
- **Type**: `rule`
- **Given**: Standard mkdisk test disk (FAT32 ESP with EFI/BOOT, EFI/ubuntu)
- **When**: `lsos` shell command is issued
- **Then**: The output identifies at least one FAT32 partition and shows bootloader names ("fallback" or "ubuntu shim" or "grub") per §8
- **Pass Condition**: `lsos` output contains both "FAT32" and either "BOOTX64" or "shim" or "grub"
- **Evidence**: Shell command transcript

### AC-7: Shell responds to help + hwdiag commands
- **Type**: `rule`
- **Given**: Boot has reached the shell prompt
- **When**: `help` then `hwdiag` are typed on serial
- **Then**: `help` lists the §10 command set; `hwdiag` re-runs diagnostics and reports a per-check severity
- **Pass Condition**: help output ≥ 8 command names; hwdiag output contains lines matching `\[(ok|warning|critical)\]`
- **Evidence**: Serial transcript of the two commands

### AC-8: grub-fix repair exercises repair-mode gate
- **Type**: `rule`
- **Given**: Boot with `--broken` disk fixture (missing EFI/BOOT/BOOTX64.EFI but EFI/ubuntu/shimx64.efi present)
- **When**: `grub-fix repair` is issued
- **Then**: The log shows "repair:" lines indicating vfs::enable_repair_mode() was called, and a subsequent `cat /EFI/BOOT/BOOTX64.EFI` (or shell equivalent) returns non-empty content
- **Pass Condition**: Log line "repair: copied" appears; second cat read succeeds
- **Evidence**: Shell transcript on the broken fixture

### AC-9: SMM OVMF NVRAM repair cycle succeeds
- **Type**: `rule`
- **Given**: `tools/run.sh --smm --broken-shim` with a persistent OVMF_VARS.fd
- **When**: Three consecutive boots run (delete-stale / create-entry / keep-existing)
- **Then**: Boot logs for each phase show the expected nvram_repair action; no SetVariable call returns a failing status outside the deliberate-delete case
- **Pass Condition**: Per-boot log contains either "nvram: deleted stale" or "nvram: created boot entry" or "nvram: entry already present", each on separate boots against one vars file
- **Evidence**: Three captured serial logs side-by-side; single VARS file path confirmed

### AC-10: Documentation files exist and match code
- **Type**: `rule`
- **Given**: Final code tree
- **When**: README.md, docs/API.md, docs/DEPLOY.md, docs/CONFIG.md, docs/CHANGELOG.md are inspected
- **Then**: Every file is non-empty; API.md lists all rust_core.h exports plus all syscall numbers; CHANGELOG.md covers milestones M0 through the final version; README current-state matches tasks.md completed items
- **Pass Condition**: All five files exist; spot-check three rust_core.h symbols in API.md; spot-check M5.5 entries in CHANGELOG
- **Evidence**: `ls -l docs/` + content grep for key terms

### AC-11: Git history is clean and pushed to remote
- **Type**: `rule`
- **Given**: Final working tree
- **When**: `git status`, `git log --oneline -20`, `git remote -v`, and `git ls-remote origin HEAD` are run
- **Then**: status is clean; commits follow `area: summary` format; origin points to a GitHub URL; remote HEAD SHA matches local HEAD SHA
- **Pass Condition**: All four checks satisfy their conditions
- **Evidence**: Captured git command outputs

### AC-12: Modularity & file-size discipline
- **Type**: `rubric`
- **Dimension**: Conformance to DESIGN.md §13.4a (small files, header comments, section banners)
- **Scale**: 1–5
- **Anchors**: 1 = multiple new files >450 lines with no headers; 3 = new files mostly ≤300 lines, some missing section banners; 5 = every new source file ≤300 lines, each has a module-map comment header, and each uses section banners for logical blocks
- **Pass Threshold**: >= 4
- **Evidence**: wc -l on new/changed files + head -n 8 inspection of each

### AC-13: Code-quality & read-only safety discipline
- **Type**: `rubric`
- **Dimension**: Read-only-first safety adherence, English-only artifact consistency, and no raw magic-hex in assembly (asm_defs.inc pipeline per §13.3)
- **Scale**: 1–5
- **Anchors**: 1 = unguarded disk write paths present; 3 = repair-mode gate exists but some comments/messages are non-English; 5 = every blk_write call site is guarded by REPAIR_MODE check; all code/comments/docs/commit messages are English; assembly contains zero raw hex constants except through asm_defs.inc
- **Pass Threshold**: >= 4
- **Evidence**: Grep for `blk_write` call sites + REPAIR_MODE gating; locale sweep over messages; grep for raw hex literals in *.S files outside the generated .inc

## Open Questions
- [ ] Q1: What is the exact GitHub remote URL the user wants to push to? (If unset, prompt during implement before pushing.)
- [ ] Q2: Does the user want surface scan to be runnable from the shell AND from boot auto-mode, or shell-only per §7? (Default: shell-only `diskhealth --scan` to keep the rescue-first iron rule.)
- [ ] Q3: Should the shell replace the idle loop (post boot-complete) unconditionally, or only when a console input device is detected? (Default: unconditional after boot-complete; headless serial always works.)
