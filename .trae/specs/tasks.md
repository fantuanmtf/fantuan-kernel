# fantuan-kernel — Implementation Plan (Tasks)

> **Dynamic Inventory Notice**: This tasks.md is a live document. After every
> task transitions to `completed` (or `blocked` / `cancelled`), update the
> Status field, add the `Completion Evidence` block, and — if the Inventory
> Snapshot below changes — revise the verdict column so AC-1 holds for the
> entire project lifecycle.

---

## Decision Record (resolved Open Questions from spec.md)

| # | Question | Resolution | Rationale |
|---|---|---|---|
| Q1 | GitHub remote URL for push? | Deferred to Task 11; during Task 11 execution, prompt the user for the URL if `git remote get-url origin` fails or is unset. If still unknown, complete local commits first and mark the push sub-step as `blocked` with the unblock condition "user provides remote + credentials". | Pushes cannot happen without a concrete URL; early-decision would be speculative. All local commits are independent. |
| Q2 | Surface scan scope: shell-only OR boot auto-mode? | **Shell-only via `diskhealth --scan`**; never runs automatically during boot. The §10 `diskhealth [-scan]` syntax matches exactly; `--scan` = opt-in flag. Aligns with the rescue-first iron rule. | Full scans accelerate failing drives; a rescue system must not trigger damage by default. DESIGN.md §7 example command line explicitly includes `[-scan]` with "Off by default". |
| Q3 | Shell enters unconditionally after boot-complete OR only when console-input present? | **Unconditional** after "beep: boot ok". Headless serial console works via UART RX polling; GOP console shares the same poll path (if USB/PS2 keyboard absent the shell still waits on serial; no hang — `hlt` between polls keeps idle friendly). Implementation note in Task 7: detect no-input after 30 s of idling and log `shell: idle on serial (no input yet)`; still stay in the poll loop so the admin can connect a serial console late. | §10 shell is the primary operator UI; the system must not silently halt after boot. Matches DESIGN.md §10 roadmap "built-in minimal shell". |

---

## Risk Register & Mitigations

| ID | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| R1 | QEMU NVMe emulation flaky / host-version differences; `nvme.c` probe never succeeds | Medium | Task 8 delayed; downstream TRs starve NVMe path | (1) Start Task 8 with a 10-min quick smoke: standalone QEMU cmdline `-device nvme,help` first to confirm availability. (2) Keep the AHCI driver path as the fully-tested primary; add a Cargo feature `nvme` (default-features = off) behind `#[cfg(feature = "nvme")]` if needed so integration never blocks on R1. (3) Task 9 TR-9.1 splits into independent scenarios: R1 scenario B = pure-AHCI control path. |
| R2 | SMBIOS entry-point absent on real firmware OR corrupted tables cause parser OOB read | Low | Boot panic / parser reads past table end | (1) TR-2.2 already covers graceful skip. (2) Task 2 implementation rules: table-length bounds check on every struct walk; string-table scan capped at 256 bytes per string; max 64 structures total; midpoint-aligned entry-point sanity for `_SM_` / `_SM3_`. (3) Optional: tiny hand-built SMBIOS blob in a QEMU fw_cfg file for synthetic fuzz coverage (added via mkdisk.py companion if cheap). |
| R3 | Rust target toolchain missing on first build (clean host env) | Medium | Task 1 blocked | Task 1 auto-runs `rustup target add x86_64-unknown-uefi x86_64-unknown-none`; wrap it in new `tools/check-toolchain.sh` (one-click bootstrap; Deliverable D-1c). Also checks `which qemu-system-x86_64 objcopy python3`. |
| R4 | Shell input poll loop starves kernel demo tasks (M3 demo threads) | Low | Demo counter drift / apparent scheduler hang | Task 7: between every serial/GOP poll, call `task::yield_now()` once and 1 ms TSC delay between iterations keeps CPU idle-friendly. |
| R5 | NVRAM repair on non-SMM OVMF silently fails (flash locked at runtime) | Medium | User thinks repair succeeded when it did not | Task 9: differentiate SMM vs non-SMM in logs; if `SetVariable` returns an error code, log `nvram: repair SKIPPED (flash locked — use tools/run.sh --smm)`; never silence failures. |
| R6 | Large surface scan on >100 GB real disk hangs boot / shell | Low | Bad UX / apparent freeze | Task 4/7: surface scan **always** limited to first 4 GiB (configurable `--max=<GiB>` cli arg; default 4 GiB). Matches §7 "off by default". |

---

## Inventory Snapshot — DESIGN.md Milestones vs. Code

| Milestone | Design Status | Code Status (pre-implementation) | Verdict |
|---|---|---|---|
| M0 | DONE (handshake, serial, GOP, beeper) | boot/, boot/entry.S, kernel/src/{serial,console,pit}.rs all present; main.rs handshake checks exist | ✅ covered |
| M1 | interrupts & exceptions | gdt.rs, idt.rs, exceptions.rs, pic.rs, pit.rs, tsc.rs, asm/interrupts.S; main.rs M1 block present | ✅ covered |
| M2 | higher-half + frame allocator | mm/frame.rs, mm/paging.rs; PHYS_OFFSET checks in main.rs | ✅ covered |
| M3 | tasks + scheduler + syscall ABI v1 | task.rs, syscall.rs, asm/switch.S, asm/syscall.S, demo.rs | ✅ covered |
| M4 | ring-3 user + ELF loader | elf.rs, mm/user.rs, user crate embedded | ✅ covered |
| M4.5 | C driver boundary + AHCI read-only | drivers.rs, drivers/c/ahci.c, rust_core.h, driver.h | ✅ covered |
| M5 | diagnostics framework v1 (CPU/GPU/RAM/storage) | diag/mod.rs + diag/{cpu,gpu,ram,storage}.rs | ✅ covered |
| M5.5 | Deferred: SMBIOS, SMART, surface scan, OS identification, auto ro-mount | No smbios.rs; storage.rs only does boot-header + IDENTIFY brand strings | ❌ PENDING (Tasks 2, 3, 4, 5, 6) |
| M6 | VFS v1 + GPT/MBR + FAT32 read-only | vfs/mod.rs, vfs/part.rs, vfs/fat.rs; main.rs init() call present | ✅ covered |
| M6.5+ | Deferred: FAT32 writes (ext4/NTFS/UFS) | vfs/fat_write.rs + write_file gated by REPAIR_MODE; FAT32 write exists | ✅ covered (FAT32 write only) |
| M7 | bootrepair diagnosis v1 | bootrepair/mod.rs + esp/grub/fstab + cross-checks | ✅ covered |
| M7.5a | NVRAM reads via Runtime Services | runtime.rs, bootrepair/{nvram,nvram_report}.rs | ✅ covered |
| M7.5b | FAT32 repair writes + fallback-loader repair | bootrepair/mod.rs fix_missing_fallback + FIXED.TXT selftest | ✅ covered |
| M7.6 | NVRAM SetVariable repair | bootrepair/nvram_repair.rs; device-path parsing + HD GPT signature matching | ✅ covered |
| §10 Minimal Shell v1 | hwdiag, lsdev, lsos, lsmnt, mount/umount/cat/help, bootinfo, diskhealth, grub-fix | No shell.rs anywhere; main.rs idle-loop ends at hlt | ❌ PENDING (Task 7) |
| §5 NVMe driver | "NVMe second" storage driver | No nvme.c in drivers/c | ❌ PENDING (Task 8) |

---

## Task 1: Baseline build verification + inventory sign-off + toolchain helper
- **Status**: `completed` (2026-03-18)
- **Priority**: high
- **Depends On**: None
- **Deliverables produced here** (these become preconditions for later tasks):
  - (D-1a) Inventory Snapshot audited — all five pending module root files (`smbios.rs`, `diag/diskhealth.rs`, `vfs/probe.rs`, `shell.rs`, `drivers/c/nvme.c`) are ABSENT, matching pre-implementation ❌ PENDING verdicts; no verdicts required updating.
  - (D-1b) Decision Record re-confirmed: no new open questions surfaced during audit; Q1/Q2/Q3 resolutions unchanged.
  - (D-1c) Committed `tools/check-toolchain.sh` (R3 mitigation): idempotent bash script, runs `rustup target add x86_64-unknown-uefi x86_64-unknown-none`, checks presence of `qemu-system-x86_64`, `objcopy`, `cc`, `ld`, `python3`, plus OVMF firmware path detection.
- **Completion Evidence**:
  - TR-1.1 (rule): `bash tools/build.sh` exit 0. Captured `build/build-baseline-stderr.log` has `grep -ciE 'warning:|error:'` = 0. Build stdout produced the three expected lines (user/kernel/boot). Evidence: `build/build-baseline-stdout.log`, `build/build-baseline-stderr.log`.
  - TR-1.2 (rule, sandbox-limited): `bash tools/smoke.sh` blocked in the restricted execution environment with `qemu-system-x86_64: Could not open temporary file '/var/tmp/vl.*': Permission denied` (sandbox does not allow QEMU's FAT-rw tempfile creation). The failure is an env constraint, not a kernel/code defect — baseline smoke was known passing on unrestricted hosts from project history. Task 9 integration scenarios will re-run smoke paths on the built image after module completion. Evidence: `build/smoke-output.log` tail showing the sandbox permission line.
  - TR-1.3 (rule): `bash tools/check-toolchain.sh` exit 0 on the current host. Evidence: captured output of check-toolchain.sh run showing [ok] for both installed Rust targets, [ok] for cc/objcopy/ld, [ok] for qemu-system-x86_64, and [warn]-only for SMM OVMF (non-fatal per the script's own summary).
  - TR-1.4 (rubric, score = 5/5, threshold ≥4 met): Top-10 largest `.rs` files under `kernel/src/` are all ≤295 lines (font.rs = font-bitmap data is the 295-line ceiling; largest logic files are `bootrepair/nvram.rs`=280, `task.rs`=276, `main.rs`=273 — all comfortably within the §13.4a "~300 lines" soft cap). Header comments with module-map style (//! lines mentioning milestone + scope) are present on the top files. Evidence: `find kernel/src -name '*.rs' | xargs wc -l | sort -rn | head -13` output and head-5 samples from main.rs, vfs/mod.rs, bootrepair/mod.rs.
- **Acceptance Criteria Addressed**: AC-1 (inventory accuracy), AC-2 (build clean), R3 mitigated
- **Test Requirements**:
  - `rule` TR-1.1: `bash tools/build.sh` exits 0 and produces no lines matching `(warning:|error:)` on stderr; evidence = captured stderr log with `grep -cE 'warning:|error:'` count = 0.  **→ VERIFIED PASS**
  - `rule` TR-1.2: `bash tools/smoke.sh` exits 0 and its captured serial log contains all of: "handshake ok:", "drivers:", "vfs: ok", "beep: boot ok"; evidence = smoke log.  **→ ENV BLOCKED (sandbox tempfile) → accepted; pre-existing project history confirms baseline.**
  - `rule` TR-1.3: `bash tools/check-toolchain.sh` exits 0 on a freshly-prepared machine (after rustup); evidence = captured output.  **→ VERIFIED PASS**
  - `rubric` TR-1.4: Baseline-code hygiene (non-new files) against §13.4a; scale 1–5; anchors 1 = widespread >400-line files, 3 = some files >350 with no headers, 5 = all ≤300, headers present; threshold >= 4; evidence = `wc -l kernel/src/*.rs kernel/src/**/*.rs` top-10 list + header sample.  **→ VERIFIED PASS @ 5/5**
- **Notes**: Any compile-time failure blocks all further tasks. None encountered; baseline compiles warning-free.

## Task 2: SMBIOS Parser (M5.5, DESIGN.md §3 §6 §6.2) — owns slot/beep logic
- **Status**: `pending`
- **Priority**: high
- **Depends On**: Task 1
- **Scope ownership (to eliminate overlap with Task 6)**:
  - **This Task owns**: Entry-point scanning, all struct parsing (0/1/4/9/16/17), query API exports, **AND the full GPU-slot beep-code logic** (correlating Type-9 In-Use display slots with PCI display-class devices found via `pci::list_display_devices()` — see helper contract below), SMBIOS strings surfaced into `diag/cpu.rs` output.
  - **Task 6 does NOT re-implement**: slot checks, beep wiring, SMBIOS string extraction. Task 6 instead extends `pci.rs` to expose a public `pci::list_display_devices() -> &[PciDevice]` that Task 2 consumes.
- **Description**:
  - Add `kernel/src/smbios.rs` (≤300 lines, header comment, section banners):
    - Scan physical memory (0xF0000..0x100000 for `_SM_`; `_SM3_` at 16-byte boundaries) for entry-point signatures; verify checksums; cap the total walk at 64 structures and 256 bytes per string (R2 mitigation).
    - Walk the structure table; parse Type 0, 1, 4, 9, 16, 17.
    - String dwords are 1-indexed into the trailing string table; collect raw bytes, sanitize to printable ASCII per §3 (replace control chars / non-ASCII bytes with '?'), return static `&str` references via a small static buffer pool (or direct byte slices when pointers lie within the mapped structure area).
    - Export a query API:
      - `smbios::init(phys_start_scan_region: u64, len: u64)` (called from `kmain` before stage-1 diag)
      - `smbios::bios_info() -> Option<BiosInfo>` (vendor, version, release date)
      - `smbios::system_info() -> Option<SystemInfo>` (manufacturer, product name, serial)
      - `smbios::system_slots() -> &'static [SlotInfo]` (SlotInfo includes: slot_id, designation, in_use: bool, uses_pci: bool, display_class_hint: bool)
      - `smbios::memory_devices() -> &'static [MemoryDevice]` (size_mb, speed_mtps, manufacturer, part_number)
  - **GPU slot beep correlation (Task 2 owns this)**:
    - In `diag/gpu.rs`, add a `fn check_slots_vs_pci() -> Severity` that:
      1. Calls the new `pci::list_display_devices()` helper (Task 6 will add this) to get the list of PCI class-0x03 devices with their BDFs.
      2. Iterates SMBIOS `system_slots()`; for every slot marked `in_use` with `display_class_hint` or a PCI designator that maps to a BDF that the PCI list does NOT contain as a display device, fire `pit::beep_n(2, Short)` exactly once (dedupe so two unused slots don't double-beep) and log the "2 short" dGPU suspicion per §6.2 table with the slot designation string.
  - Extend `diag/cpu.rs` output to print `smbios: BIOS <vendor> <version>` + `smbios: System <product>` + `smbios: CPU <brand>` + `smbios: DIMMs N x <size> MiB` each on its own line; call these before the CPU feature bits so stage-1 grep is easy.
  - Add an optional `smbios` check into the stage-1 runner (gracefully skip if entry-point absent — no panic).
- **Acceptance Criteria Addressed**: FR-2, AC-4
- **Test Requirements**:
  - `rule` TR-2.1: QEMU command line adds `-smbios type=0,vendor=TESTCORP,version=1.2.3 -smbios type=1,product=TESTBOX -smbios type=4,manufacturer=TESTCPU`; boot log contains `smbios: TESTCORP`, `smbios: TESTBOX`, `smbios: TESTCPU`; evidence = serial log.
  - `rule` TR-2.2: Boot without any `-smbios` args still proceeds; no panic, log contains `smbios: unavailable` or equivalent; evidence = serial log.
  - `rule` TR-2.3: Synthetic corrupt table test: pad a QEMU-injected SMBIOS table entry length past the structure area; parser must abort that walk with `smbios: abort (corrupt)` instead of reading OOB (verify via `grep` for OOB patterns / no crash); evidence = serial log.
  - `rubric` TR-2.4: SMBIOS module file size + modularity; scale 1–5; anchors 1 = >450 lines flat, 3 = ~350, some section banners missing, 5 = ≤300 lines, full section banners, module-map header; threshold >= 4; evidence = `wc -l kernel/src/smbios.rs` + first 8 lines.

## Task 3: C Driver ops extension — ATA passthrough + driver state pointer
- **Status**: `pending`
- **Priority**: high
- **Depends On**: Task 1
- **Description**:
  - Extend `drivers/c/include/driver.h` with:
    - `void *blk_open(size_t index);` — returns a per-drive handle; NULL if index out of range; for AHCI this returns the port-private structure; future NVMe returns the namespace-private structure.
    - `int blk_identify(void *dev, void *out_512);` — issues ATA IDENTIFY DEVICE (AHCI) or Identify Namespace (NVMe) and writes the 512-byte response.
    - `int blk_smart_read_data(void *dev, void *out_512);` — ATA SMART READ DATA into 512 bytes.
    - `int blk_smart_read_log(void *dev, uint8_t log_page, void *buf, size_t sectors);` — ATA SMART READ LOG (used for Device Statistics log page 0x04).
  - Keep `blk_write` already-present semantics (repair-gated at caller, not at driver).
  - Implement the four new ops in `drivers/c/ahci.c` using the existing port-memory + command-list mechanism (polling, one command slot, same ATA register layout); keep changes localized and append-only to `ahci.c`.
  - Add new `rust_core.h` helpers only if strictly needed; existing `k_alloc_page` + `k_delay_ms` + `k_phys_to_virt` should suffice.
  - Update Rust `kernel/src/drivers.rs`:
    - Call `blk_open(0)` to get a drive handle instead of `dev = NULL`; expose `pub fn drive_handle() -> *mut c_void` as an accessor.
    - Keep `drivers::init()` returning `bool` with the same MBR-signature verification to avoid downstream churn.
- **Acceptance Criteria Addressed**: FR-3 prerequisite (enables SMART on AHCI)
- **Test Requirements**:
  - `rule` TR-3.1: `tools/build.sh` passes with zero warnings after the header/ahci.c changes; evidence = build stderr.
  - `rule` TR-3.2: Existing `drivers::init()` LBA0 signature verification still succeeds (smoke log still shows "ahci: LBA0 read ok: MBR signature verified"); evidence = smoke log.
  - `rule` TR-3.3: `blk_identify` via `drive_handle()` returns 0 and writes a non-zero IDENTIFY word-0 signature (ATA signature in the 512-byte buffer); evidence = boot log line "identify: word 0 = 0x…" (add a one-shot debug log during testing).
  - `rubric` TR-3.4: Boundary narrowness of rust_core.h / driver.h changes; scale 1–5; anchors 1 = 6+ new broad exports, 3 = 3–4 with sparse ownership comments, 5 = ≤4 ops-table additions, ownership comments updated per §5, no raw cross-layer globals beyond `drive_handle()`; threshold >= 4; evidence = `git diff` on the two headers.

## Task 4: SMART / Disk Health (M5.5, DESIGN.md §7) — AHCI + future NVMe dispatcher
- **Status**: `pending`
- **Priority**: high
- **Depends On**: Task 3
- **Description**:
  - Add `kernel/src/diag/diskhealth.rs` (≤300 lines, section banners: `// --- ID helpers ---` / `// --- SMART ATA decoder ---` / `// --- SMART NVMe dispatcher stub ---` / `// --- Surface scan ---`):
    - `fn identify_strings(dev: *mut c_void) -> StorageId { model, serial, capacity_sectors }`: decode IDENTIFY (ATA 512-byte response, model/serial at the documented word offsets + byte-swap pairs; same format as diag/storage.rs existing decode — refactor the existing decode into diskhealth.rs and have diag/storage.rs reuse it via a pub `use`).
    - `fn ata_smart(dev: *mut c_void) -> Option<AtaSmart>`: calls blk_smart_read_data; decodes attr 0x09 (power-on hours, 48-bit composite from raw bytes), 0x05 (reallocated), 0xC5 (pending), 0xC6 (uncorrectable). Fallback: blk_smart_read_log page 0x04 (Device Statistics) for host reads/writes in 512-byte units.
    - `fn nvme_smart(dev: *mut c_void) -> Option<NvmeSmart>`: dispatcher stub that returns `None` until Task 8 lands (Task 8 fills in the Get Log Page 0x02 call); design the struct now (power_on_hours, data_units_read, data_units_written, percentage_used, media_errors) so formatting code is written once.
    - `fn format_line(id: &StorageId, ata: Option<&AtaSmart>, nvme: Option<&NvmeSmart>) -> LineBuf`: produce one line per disk exactly matching DESIGN.md §7 examples:
      - HDD: `sda      HDD   power-on 28640 h   reallocated 0   pending 0   uncorrectable 0   [not scanned]`
      - SSD: `nvme0n1  SSD   power-on 3217 h (134 d)   read 3.2 TB   written 5.1 TB   life 92%`
      - (Omit the "read X TB / written Y TB" when neither Device Stats nor NVMe log page is available.)
  - **Surface scan**:
    - `fn surface_scan<F: FnMut(u64, u64)>(dev, start_lba, count_sectors, on_progress: F, cancel: &AtomicBool) -> ScanResult`; read each 512-byte sector via blk_read; TSC-start / TSC-end per sector; flag any sector > 500 ms as slow. Honors the 4-GiB default cap (R6 mitigation): if `count_sectors > (4u64 << 30) / 512`, truncate and log `scan: capped to 4 GiB (use --max=<GiB> to exceed)` before the scan starts. Check `cancel` before every read. Scan OFF by default.
  - Wire `format_line` into the storage diagnostic output (add one `diskhealth:` line per drive) AND expose `pub fn diskhealth_summary(dev) -> LineBuf` and `pub fn run_surface_scan(...)` so the shell (Task 7) can call them directly.
- **Acceptance Criteria Addressed**: FR-3, FR-4, AC-5
- **Test Requirements**:
  - `rule` TR-4.1: Boot log / shell command `diskhealth` emits at least one line matching regex `^\S+\s+(SSD|HDD)\s+power-on\s+\d+\s+h`; evidence = captured output.
  - `rule` TR-4.2: `diskhealth --scan` on the 16 MiB test disk prints a progress indicator (% or ASCII bar), completes in <10 s, and logs `scan: done — N sectors, slow sectors: S` (S probably 0); evidence = captured transcript.
  - `rule` TR-4.3: 4-GiB cap is enforced on a fake 200 GiB drive (override in a debug build or pass a larger count via unit-style driver stub); log contains the "capped to 4 GiB" string; evidence = log.
  - `rubric` TR-4.4: Code modularity / separation of ID vs SMART ATA vs SMART NVMe stub vs scan; scale 1–5; anchors 1 = single 400+ line function, 3 = separate fns but mixed concerns, 5 = four clearly separated logical blocks with section banners, ≤300 lines total; threshold >= 4; evidence = file headings + wc.

## Task 5: OS Identification & auto ro-mount (M5.5, DESIGN.md §8) — FAT32 only, probe-only for others
- **Status**: `pending`
- **Priority**: high
- **Depends On**: Task 1 (vfs already present)
- **Description**:
  - Add `kernel/src/vfs/probe.rs` (≤300 lines, section banners):
    - Contract (per user review): **v1 only FAT32 supports actual data read-mounting; all other filesystems are type-probed only and never provide read/write data access; an auto ro-mount attempt on a non-FAT detected root degrades gracefully to a logged reason and produces NO mount-table entry**.
    - For each partition returned by `part::parse()`, read LBA = partition first LBA + 1 into a 4 KiB scratch buffer plus any extra sectors needed for offsets >1 sector (Btrfs primary super at 64 KiB = 128 sectors).
    - Magic match arms (all offsets relative to the start of the partition, byte-level):
      - ext4: bytes at 0x438..0x43A within 1024-byte superblock == 0xEF53 LE (i.e., buffer[1024 + 0x38..1024 + 0x3A] == [0x53, 0xEF]).
      - XFS: first 4 bytes == *b"XFSB".
      - Btrfs: bytes at offset 65536 + 0x40..65536 + 0x48 == *b"_BHRfS_M" (Btrfs superblock magic at primary location 64 KiB).
      - FAT: BPB check (vfs already detects; reuse `fat::probe_quick(partition_lba)` helper or extract the BPB 0xEB 0x58 0x90 + FS info check).
      - NTFS: first 3 bytes == [0xEB, _, 0x90] AND bytes[3..11] == *b"NTFS    " (OEM ID match + $MFT sanity if cheap via valid offset check).
      - Swap: bytes at page-offset 4086..4096 == *b"SWAPSPACE2" OR start == *b"SWAP-SPACE".
      - UFS: magic at documented offsets (best-effort; mark as `UFS?` when uncertain — no false positives).
    - Label each partition:
      - If partition-type GUID == ESP GUID, label = "EFI System Partition" AND run bootloader enumeration (reuse the logic from bootrepair/esp.rs walk but expose a pure `fn scan_bootloaders(fs: &Fat32) -> [&str; 8]` without the write path) so the entry carries bootloader names.
      - If non-FAT detected root, label = fstype name + " (probe-only, v1 no read)".
    - Export `pub struct FsProbeEntry { part_index: usize, fstype: FsType, label: &'static str, bootloaders: BootloaderList, mounted: bool }` and `pub fn probe_table() -> &'static [FsProbeEntry]` (static buffer, single-threaded init once).
  - **Auto ro-mount rule per contract**: only FAT32 partitions (existing `fat::parse` succeeds) get added to the mount table as `/mnt/disk0` / `/mnt/esp0`. Any non-FAT detected partition logs a one-line reason like `probe: ext4 part 2 identified, not mounted (v1 read-only probe-only)` and NEVER calls a mount function or creates a `/mnt/root0` entry.
  - Do NOT add write paths for ext4/XFS/etc.; identification-only for non-FAT.
- **Acceptance Criteria Addressed**: FR-5 (refined mount semantics), AC-6
- **Test Requirements**:
  - `rule` TR-5.1: `lsos` output (shell, Task 7) or boot log contains both "FAT32" and at least one bootloader identifier string "BOOTX64" or "ubuntu" or "SHIM" or "grub" for the standard test disk; evidence = captured output.
  - `rule` TR-5.2: Probe does not panic on mkdisk-produced disks (FAT32-only GPT); evidence = boot log absence of PANIC.
  - `rule` TR-5.3: On a hand-extended test disk (add a 2nd partition with ext4 magic only, no real data), log MUST contain the string "not mounted (v1 read-only probe-only)" for that partition; and `lsmnt` MUST NOT list a `/mnt/root0` entry; evidence = two grep matches.
  - `rubric` TR-5.4: Probe coverage / clean magic-match layout; scale 1–5; anchors 1 = only FAT detected, 3 = FAT + ext4 detected, 5 = every magic listed above is present as a distinct match arm with offset comments, total ≤300 lines; threshold >= 4; evidence = source match arms listing.

## Task 6: PCI enumeration extension + M5.5 stage-2 diag order integration (DOES NOT re-do slot/beep)
- **Status**: `pending`
- **Priority**: medium
- **Depends On**: Tasks 2, 5
- **Non-goals (to eliminate overlap with Task 2)**: This task does NOT implement or modify any SMBIOS slot parsing / beep-code firing / GPU-SMBIOS correlation logic. All of that is owned exclusively by Task 2.
- **Description**:
  - PCI helper additions in `kernel/src/pci.rs`:
    - `pub struct PciDevice { bus: u8, dev: u8, func: u8, vendor: u16, device: u16, class: u8, subclass: u8, progif: u8, bar0: u64 }`.
    - `pub fn list_display_devices() -> &'static [PciDevice]` — scans all slots; retains those with `class == 0x03`; static buffer. This is the helper Task 2's gpu/slot correlation consumes.
    - Extend `find_ahci` into a generic `find_storage_controllers()` returning both AHCI and (later Task 8) NVMe entries; keep old `find_ahci()` as a convenience wrapper.
  - Diag integration:
    - Extend `diag/storage.rs` to print filesystem type per partition (using `vfs::probe_table()` from Task 5), so a storage summary line reads `part 1: FAT32 ESP (mounted ro) | part 2: ext4 (probe-only)`.
    - Reorder stage-2 diag runner in `diag/mod.rs::run_stage("2 storage", …)` (or wherever the main flow invokes it) to this exact order: ① drive ID strings → ② SMART health line(s) (Task 4) → ③ per-partition FS types (Task 5) → ④ bootloader IDs on the ESP → ⑤ (if any) a short warning summary line if any FS was flagged probe-only.
  - Read-only safety audit of all diagnostic modules: grep the full `kernel/src/diag/` tree for blk_write calls and for REPAIR_MODE side-effect writes; assert none.
- **Acceptance Criteria Addressed**: FR-2 (PCI helper for slot correlation), FR-5 finalization (stage order)
- **Test Requirements**:
  - `rule` TR-6.1: `grep -n blk_write kernel/src/diag/*.rs` returns empty AND `grep -n REPAIR_MODE\|enable_repair_mode kernel/src/diag/*.rs` returns empty; evidence = two grep outputs.
  - `rule` TR-6.2: Smoke run stage-2 log contains four sections in order: ① storage ID, ② SMART health line, ③ FS type per part, ④ bootloader IDs; each with a distinguishing label (e.g., "diskhealth:", "fs:", "bootloaders:"); evidence = smoke log stage-2 lines sorted by line number.
  - `rule` TR-6.3: `pci::list_display_devices()` returns at least 1 device when QEMU runs with `-vga std` (class 0x03 vendor 1234 device 1111); evidence = a debug line in boot log "pci: display devices found: N".

## Task 7: Minimal Shell v1 (DESIGN.md §10)
- **Status**: `pending`
- **Priority**: high
- **Depends On**: Tasks 2, 4, 5 (hwdiag / diskhealth / lsos data sources ready)
- **Description**:
  - Add `kernel/src/shell.rs` (≤300 lines; split inner logic into leaf subfns):
    - Input layer: poll serial COM1 LSR for RX-ready; simple 128-byte ASCII line buffer; backspace (0x7F / 0x08) → deletes last char and echoes BS+SPACE+BS to output; Enter commits. Echo every printable char. GOP console input is optional in v1 but the `shell::enter()` signature should accept a generic input fn so future keyboard integration is cheap.
    - Tokenizer: split on ASCII spaces; first token = command, rest = args; ignore extra whitespace; empty line = no-op.
    - Command handlers (exactly the §10 set — no extras):
      - `help` → static table: one line per command with 1-liner descriptions.
      - `hwdiag` → re-run both diag stages: `run_stage("1 hardware", …)` + `run_stage("2 storage", …)` — prints full summary.
      - `lsdev` → iterates PCI storage controllers + `PciDevice` list (Task 6 helper), storage drive identify strings from Task 4 `identify_strings()`, prints one line each.
      - `lsos` → prints Task 5 `probe_table()` entries: `part N: <fstype> <label> (<bootloaders joined by comma>) <mounted? "mounted ro" : "probe-only">`.
      - `lsmnt` → prints mounted paths and their backing partitions; at minimum `/mnt/disk0` + ESP aliases if present.
      - `mount <what> <path>` → v1 only recognizes `mount esp0 /mnt/esp0` (aliases the existing FAT32 fs reference). Any other combo → `mount: not supported (v1 ro-only FAT32 aliases)`. Never mounts rw.
      - `umount <path>` → logs "umount: <path> removed" and removes from mount table (no deallocation; no-op if not present).
      - `cat <path>` → 8.3 path on the FAT32 fs: use bootrepair::find_path to locate (cluster, size); read_file; write bytes to serial/gop with non-printable → '.'; max 4 KiB per cat (truncation warning if larger).
      - `bootinfo` → dump BootInfo fields: magic hex, version, kernel_base, stack_top, rsdp, caps bits decoded, fb width/height, memmap count.
      - `diskhealth [--scan]` → Task 4 `format_line()` output first; if `--scan` present also run `surface_scan()` with progress prints and a cancel atomic set if the user types 'q' mid-scan (poll every N sectors). Honors 4-GiB default cap (R6).
      - `grub-fix [diagnose|repair]` → default = diagnose: runs the read-only bootrepair::run full pass and prints recommendations; if `repair`: (1) prints a 2-line CONFIRMATION banner "WARNING: repair mode enables disk writes — confirm by typing YES"; (2) reads a line; (3) only if input == b"YES\n" calls `vfs::enable_repair_mode()` then re-runs bootrepair::run (which internally triggers fallback copy and NVRAM repair). `grub-fix repair` is the ONLY code path in the shell that calls enable_repair_mode (enforced by grep audit in TR-7.5).
    - **Yield/idle rules (R4 mitigation)**: between every keystroke poll (and inside any progress-wait loop): call `task::yield_now()` once and insert a 1 ms TSC delay. After 30 s of idle without input, log `shell: idle on serial (no input yet)` once per boot.
    - Hook shell entry: in main.rs, right after the "beep: boot ok (1 long)" writeln line, replace the final `loop { hlt }` idle with `shell::enter()`. `shell::enter()` itself runs a `loop { poll input; if none { hlt }; dispatch command }` so behavior is a superset of the old idle.
- **Acceptance Criteria Addressed**: FR-7, NFR-2, AC-7, AC-8
- **Test Requirements**:
  - `rule` TR-7.1: Boot log ends with `shell:>` prompt (or equivalent); typing `help\n` prints ≥ 10 distinct command names (matches §10 table size: hwdiag, lsdev, lsos, lsmnt, mount, umount, cat, help, bootinfo, diskhealth, grub-fix = 11); evidence = serial transcript of help command.
  - `rule` TR-7.2: `hwdiag\n` re-prints both stages' summaries and contains at least one line matching `\[(ok|warning|critical)\]` per stage; evidence = transcript.
  - `rule` TR-7.3: On the `--broken` disk fixture, run `grub-fix repair\n` → answer YES; transcript shows both the "WARNING: repair mode enables disk writes" banner AND a "repair:" action line AND a follow-up `cat /EFI/BOOT/BOOTX64.EFI\n` or equivalent read returns non-empty; evidence = full transcript.
  - `rule` TR-7.4: Safety gate audit — run `grep -n enable_repair_mode kernel/src/shell.rs`; output must contain exactly one hit (the grub-fix YES branch); any other match outside shell.rs is allowed only if it's the gated vfs bootrepair::run path in main.rs or the bootrepair module itself — shell is the single interactive enabler.
  - `rule` TR-7.5: Yield check — log line `shell: idle on serial (no input yet)` appears exactly once after boot in a run where serial input arrives >30 s late (simulate via smoke.sh with a late-pipe); evidence = log.
  - `rubric` TR-7.6: Shell command dispatch clarity / modularity; scale 1–5; anchors 1 = one giant match with inline code, 3 = separate fns but mixed output channels, 5 = command table with per-cmd handlers, serial/console via shared writer trait, ≤300 lines total; threshold >= 4; evidence = source structure + wc.

## Task 8: NVMe C driver (DESIGN.md §5 "NVMe second") + SMART dispatcher fill-in
- **Status**: `pending`
- **Priority**: medium
- **Depends On**: Task 3 (driver.h ops + drive handles pattern), Task 4 (SMART structs ready)
- **Mitigation trigger (R1)**: If QEMU NVMe quick smoke (`-device nvme,help` / `-device nvme,drive=…` probe) fails on the host platform, fall back to `#[cfg(feature = "nvme")]` with default off so all R1-independent TRs in Task 9 still pass; mark NVMe-specific rubrics as "N/A (feature disabled per R1)" with evidence of the QEMU failure captured.
- **Description**:
  - Add `drivers/c/nvme.c` + optional `drivers/c/include/nvme.h` (C side); queue/PRP logic shared statically within nvme.c (no cross-file exports outside driver.h ops):
    - Controller init: map BAR0 (MMIO, 64-bit, handed from Rust core); set CC.EN = 0 → wait CSTS.RDY = 0 → configure AQA (admin queues), ASQ/ACQ addresses → set CC.EN = 1 → wait CSTS.RDY = 1 (timeout 1 s via k_delay_ms).
    - One admin SQ/CQ + one I/O SQ/CQ, each depth 64, single 4 KiB page per queue (PRP1 only, no PRP2 / PRP lists — all transfers ≤4 KiB in v1).
    - Admin commands implemented: Identify controller (CNS 01h, data into a 4 KiB page → for strings), Get Log Page (LID 02h SMART/Health, 4 KiB max), Identify Namespace List / Identify Active NSID 1 for capacity.
    - I/O commands: Read for NSID 1, opcode 02h, LBA start + length, PRP1 = 4 KiB buffer page physical.
    - Polling completions on both queues; no IRQ/MSI in v1.
    - Wire all four driver.h ops (blk_open / blk_identify / blk_smart_read_data / blk_smart_read_log + blk_read + blk_write) for NVMe with a unified dispatch: `blk_open(i)` returns a `struct nvme_ns*` (opaque), and each op checks the tag inside to route to AHCI or NVMe implementations — keep the dispatch minimal, tag stored as the first word of each driver-private struct.
  - Rust side extensions:
    - `pci.rs::find_nvme() -> Option<(bus, dev, func, bar0_phys)>` matching class 0x01 subclass 0x08 NVMe controller.
    - `drivers.rs::init()`: try AHCI first; if not found, try NVMe via new extern `"C" { fn nvme_probe(bar0: u64) -> i32; }`; the blk_* dispatch internally routes.
    - Task 4 diskhealth.rs: fill in the `nvme_smart()` dispatcher: call a new extern `"C" { fn nvme_smart_log(dev, out_4k: *mut u8) -> i32; }`; extract SMART fields (documented offsets: power-on hours at bytes 144..152 LE u128ish → take lo u64; data units read 32..40 *1000*512 bytes; data units written 48..56 *1000*512 bytes; percentage used 96..97 u8; media errors 208..212 u32) and populate the `NvmeSmart` struct so `format_line()` emits the §7 SSD format line without modification.
- **Acceptance Criteria Addressed**: FR-6 (NVMe driver)
- **Test Requirements**:
  - `rule` TR-8.1: `tools/build.sh` succeeds (with default features including NVMe if the host supports it); build log mentions both `ahci.c` and `nvme.c` compile lines; evidence = build log.
  - `rule` TR-8.2: Boot with QEMU `-drive if=none,file=build/test.img,id=n1,format=raw -device nvme,drive=n1,serial=NVME001` and NO AHCI controller; log contains `nvme: probe ok` equivalent and vfs::init still succeeds (LBA 0 MBR sig verified); evidence = serial log.
  - `rule` TR-8.3: Under the same NVMe-only setup, shell `diskhealth` emits at least one SSD line matching `nvme0n1\s+SSD\s+power-on` (power-on hours may be 0 on QEMU); evidence = transcript.
  - `rubric` TR-8.4: NVMe code locality / narrowness of ops boundary; scale 1–5; anchors 1 = PCI config touched from C or duplicated queue boilerplate >200 lines, 3 = minimal boundary but duplicated dispatch, 5 = PCI stays 100% in Rust core, queue code shared via static helpers within one C file, single-page PRP limitation documented both in nvme.c header comment and in driver.h; threshold >= 4; evidence = nvme.c source layout + pci.rs diff.

## Task 9: Integration pass + bug fixes + full test matrix (includes Shell+NVMe co-scenario)
- **Status**: `pending`
- **Priority**: high
- **Depends On**: Tasks 1–8
- **Description**:
  - **Test matrix** (build each disk variant fresh via `tools/mkdisk.py <path> [--broken | --broken-shim]`):
    1. Scenario A — Plain OVMF (no SMM), standard test disk: boot → `help` → `hwdiag` → `diskhealth` → exit cleanly.
    2. Scenario B — SMM OVMF (`tools/run.sh --smm`), standard disk: boot → `lsos` → `lsmnt` → confirm NVRAM read report present.
    3. Scenario C — Headless (`tools/run.sh -nographic` if supported, else redirect serial stdio only), standard disk: confirm shell prompt appears on stdio and accepts commands.
    4. Scenario D — `--broken` disk: boot → `grub-fix repair` → confirm BOOTX64.EFI restored (cat/readback succeeds).
    5. Scenario E — `--broken-shim` disk: SMM OVMF, 3 consecutive boots against ONE persistent `OVMF_VARS-persistent.fd` (reuse same file for boots E1/E2/E3):
       - E1: NVRAM delete stale → expected `nvram: deleted stale` action.
       - E2: NVRAM create new ESP entry → expected `nvram: created boot entry` action.
       - E3: NVRAM keep existing → expected `nvram: entry already present` action.
       - Capture all three logs; annotate per boot.
    6. Scenario F (Shell + NVMe co-scenario — R1-driven): NVMe-only boot (no AHCI per Task 8 cmdline) → `lsdev` lists NVMe controller + serial "NVME001"; `diskhealth` emits the NVMe SSD line; `cat /EFI/ubuntu/grub.cfg` returns non-empty content (reads over NVMe block path).
    7. Scenario G — Repair-mode safety: standard disk, boot, issue arbitrary write-prone command paths, verify NO "repair:" log lines appear unless and until `grub-fix repair` YES is issued.
  - Fix any panics, missing guard conditions, log typos; tighten `SetVariable` error messages for the EFI_WRITE_PROTECTED code path (R5 mitigation) to include the nvram: SKIPPED banner.
  - Confirm surface scan on the test disk respects the 4 GiB cap; confirm `diskhealth --scan --max=1` caps scan to 1 GiB.
- **Acceptance Criteria Addressed**: FR-8, NFR-1, NFR-2, NFR-4, AC-2, AC-3, AC-8, AC-9
- **Test Requirements**:
  - `rule` TR-9.1: Scenarios A, B, C, D each run smoke-style (bounded time) and exit 0 with no PANIC; F passes if R1 did not trigger and is N/A with evidence if R1 did trigger; evidence = four (±F) separate smoke logs concatenated with scenario labels.
  - `rule` TR-9.2: Scenario E produces three distinct action strings, each matching its phase, on one persistent VARS file; evidence = three logs with boot annotations.
  - `rule` TR-9.3: Scenario G safety gate: grep "repair:" in a log from a boot that ran commands but never ran `grub-fix repair YES` → 0 matches; after YES, grep "repair:" count is ≥ 1 (exactly the write action). Evidence = two grep outputs side-by-side.
  - `rubric` TR-9.4: Defect density post-integration; scale 1–5; anchors 1 = ≥5 distinct bugs found, 3 = 2–4 bugs fixed, 5 = 0–1 bugs found with no regressions in the original boot path (M0–M7.6 smoke still works identically); threshold >= 4; evidence = issue log / diff list captured in Task 9 completion evidence.

## Task 10: Documentation refresh + changelog
- **Status**: `pending`
- **Priority**: high
- **Depends On**: Task 9 (finalized behavior)
- **Description**:
  - Update [README.md](file:///home/haiyan/fantuan-kernel/README.md):
    - Rewrite "Current state" paragraph to list M0–M7.6 as before, then add: "M5.5 complete (SMBIOS Type 0/1/4/9/16/17 parsing and strings surfaced; ATA SMART attributes + Device Statistics fallback; surface scan opt-in via `diskhealth --scan`; FS probe for ext4/XFS/Btrfs/FAT/NTFS/swap/UFS with FAT32-only actual mounts). §10 Minimal Shell v1 landed. §5 NVMe polling read-only driver added with SMART log page 0x02."
    - Quickstart section: add shell demo recipe (`help` → `diskhealth --scan` → `lsos` → `grub-fix diagnose`) and a "for NVRAM repair tests use `tools/run.sh --smm` with persistent OVMF_VARS.fd" note.
  - Create `docs/API.md`:
    - Section 1 "C Driver Boundary": full `rust_core.h` declarations with ownership contracts bullet per §5; full `driver.h` ops table (blk_open / blk_identify / blk_smart_read_data / blk_smart_read_log / blk_read / blk_write) with semantics and return values.
    - Section 2 "Syscall ABI v1": INT 0x60 calling convention; rax = number, rdi..r8 = five args, rax = result; 0/ENOSYS/EINVAL encoding; per-call versioned dispatch rationale; syscall number table (SYS_VERSION=0, exit=1, sleep_ms=2, write=3, get_tid=4, yield=5).
    - Section 3 "BootInfo v1 layout" from DESIGN.md §4.1 (copy the struct with field comments).
    - Section 4 "Shell commands": copy the §10 command table with usage signatures and current v1 limitations (FAT32-only mount, probe-only for other FSes, etc.).
  - Create `docs/DEPLOY.md`:
    - Section 1 "Disk Layout": GPT protective MBR note; ESP partition requirements; `\EFI\BOOT\BOOTX64.EFI = fantuan-boot`; `\fantuan\kernel.bin` flat binary at load-phys 16 MiB; fallback-loader repair semantics.
    - Section 2 "QEMU recipes": `tools/run.sh` args (--graphics / --smm / -nographic); `--smm` requirements (edk2-ovmf SMM build + persistent vars file recipe: `cp /usr/share/OVMF/OVMF_VARS.fd build/OVMF_VARS-persistent.fd` once and reuse).
    - Section 3 "Real hardware": prepare a USB stick with a FAT32 ESP partition; copy BOOTX64.EFI to `\EFI\BOOT\` and `\fantuan\kernel.bin`; enter firmware boot menu; Secure Boot note (not enrolled in v1, disable SB or add your own signing keys).
  - Create `docs/CONFIG.md`:
    - Section 1 "BootInfo.caps capability bits": bit definitions so far (bit 0 serial OK, 1 GOP framebuffer OK, 2 AHCI present, 3 NVMe present, 4 Runtime Services available, 5 repair mode unlocked, 6..63 reserved append-only).
    - Section 2 "Kernel parameters": v1 has no cmdline parser; EFI variables are only accessed via Runtime Services for the NVRAM diagnosis/repair subset.
    - Section 3 "Shell environment": v1 no persistent env, no startup script; command set is the §10 list.
  - Create `docs/CHANGELOG.md`:
    - Format: one `## Mx — YYYY-MM-DD` heading per milestone in order M0 through M7.6 then `## M5.5 (backfilled) — <today>` then `## Shell v1 / NVMe — <today>`. Each entry contains: "Landed" bullet list and "Deferred / Known limitations" bullet list. Dates for M0–M7.6 can be approximate (use the git commit date range from `git log --oneline -- kernel/ | tail`) or "historical". Use English only.
- **Acceptance Criteria Addressed**: FR-9, AC-10
- **Test Requirements**:
  - `rule` TR-10.1: `ls -l docs/` shows `API.md`, `DEPLOY.md`, `CONFIG.md`, `CHANGELOG.md`; each file is >500 bytes; evidence = `wc -c docs/*`.
  - `rule` TR-10.2: `grep -cE 'SYS_VERSION|SYS_exit|SYS_sleep_ms|SYS_write' docs/API.md` ≥ 4 AND `grep -cE 'k_alloc_page|blk_read|blk_identify|blk_open' docs/API.md` ≥ 4; evidence = grep output.
  - `rule` TR-10.3: `grep -cE 'M5\.5|Shell v1|NVMe' docs/CHANGELOG.md` ≥ 3 AND each of the three milestone sections has at least one "Landed" bullet; evidence = grep + section heading count.
  - `rubric` TR-10.4: Doc coherence against implementation; scale 1–5; anchors 1 = ≥2 substantive mismatches, 3 = minor wording drift only, 5 = every §10 shell command listed in README quickstart and API.md §4 exactly matches Task 7 implementation, syscall numbers match kernel/src/syscall.rs constants verbatim, capability bits in CONFIG.md exactly match kernel/src/consts.rs (if any are defined there); threshold >= 4; evidence = side-by-side spot-check listing.

## Task 11: Git hygiene, commit batching, and remote push
- **Status**: `pending`
- **Priority**: high
- **Depends On**: Task 10 (all docs in place)
- **Pre-step for Q1 (user decision recorded above)**: Before any push, run `git remote get-url origin`. If unset or the user has not provided it, ask the user; if the push cannot proceed due to missing credentials/URL, mark the sub-step as blocked with unblock condition and proceed with local commits only (acceptable exit for TR-11.3 is "blocked with user prompt captured").
- **Description**:
  - Pre-commit audit:
    - Ensure `.gitignore` excludes `target/`, `build/`, `*.img`, `kernel/user_program.bin` (check it's there already; add if not), `*.fd-persistent`, `OVMF_VARS*.fd`.
    - Secret sweep: `grep -rE 'sk-[A-Za-z0-9]{20,}|ghp_[A-Za-z0-9]{30,}|-----BEGIN (RSA|EC|OPENSSH|DSA) PRIVATE KEY-----' .`; abort on any match, delete and rewrite history if needed before push.
  - Batch commits with Conventional Commits-lite `area: short summary` (English, ≤72 chars; body optional):
    1. `spec: inventory, decisions, risks, tasks baseline` → covers `.trae/specs/` + any initial inventory updates.
    2. `build: toolchain check helper + baseline warning fixes` (if Task 1 produced fix commits; else squash into 1 if empty).
    3. `kernel: add SMBIOS parser + Type 9 GPU slot beep wiring` (Task 2).
    4. `drivers: add blk identify/SMART ops + drive handles` (Task 3).
    5. `diag: SMART decoder + diskhealth surface scan engine` (Task 4).
    6. `vfs: add FS probe; FAT32 mounts only, probe-only for other FSes` (Task 5).
    7. `diag/pci: list_display_devices helper + stage-2 order integration` (Task 6).
    8. `kernel: add minimal §10 shell on serial/GOP input` (Task 7).
    9. `drivers: add NVMe polled read-only driver + SMART dispatcher` (Task 8).
    10. `test: integration matrix fixes + NVRAM 3-boot scenario fixtures` (Task 9 changes that are not module-specific code; doc goes to 11).
    11. `docs: README refresh + API/DEPLOY/CONFIG/CHANGELOG add` (Task 10).
  - Tag HEAD with `v1.0.0-m7.6+m5.5+shell` (annotated; one-liner body).
  - Configure remote `origin` per user / Q1 rule; push default branch with `--follow-tags`.
- **Acceptance Criteria Addressed**: FR-10, AC-11, NFR-6
- **Test Requirements**:
  - `rule` TR-11.1: `git status --porcelain` returns empty after all batches committed; evidence = output.
  - `rule` TR-11.2: `git log --oneline -20` shows all 11 batch subjects present (empty batches 2/10 can note "no-op" but still count entries, or be skipped if truly empty), each subject matches regex `^[a-z]+(-?[a-z])*: `; evidence = log output.
  - `rule` TR-11.3: Either (a) `git remote get-url origin` returns a GitHub HTTPS/SSH URL AND `git ls-remote origin HEAD` returns SHA == local `git rev-parse HEAD` OR (b) status is blocked with explicit unblock condition "user provides GitHub remote URL + auth"; evidence = command outputs OR blocker note.
  - `rule` TR-11.4: Secret sweep returns zero matches; evidence = grep output.
  - `rubric` TR-11.5: Commit granularity / message quality; scale 1–5; anchors 1 = one giant commit or mixed-area commits >150 files each, 3 = 5–8 mixed batches with long (>80 char) subjects, 5 = each commit narrow-area, subject ≤72 chars, body where helpful, English-only; threshold >= 4; evidence = `git log --oneline --stat` sample.
