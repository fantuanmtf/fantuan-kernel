# M12 Design — Disk Tooling, NTFS, GPU Probe, Virtualization Detection

> Status: design of record for v0.0.4, written before code (DESIGN §13.5).
> Roadmap: `ROADMAP_v0.0.2+.md` §4.

## 1. Goal

Four hardware-facing capabilities for field rescue:

1. **Disk imager** — clone a whole disk to another disk or an image file,
   verified by hash, write-gated like every repair action.
2. **NTFS read-only** — inspect Windows files without touching them
   (boot-chain repair stays out of scope; WinPE is recommended).
3. **AMD GPU probe** — report identity/link/thermal state conservatively,
   never guess "good/bad" beyond the measured signals.
4. **Virtualization V1** — detect what the CPU/firmware offers and print the
   2010->modern compatibility matrix that M14/M15 build on.

## 2. Disk imager

- **Scope**: block-level copy, sector-aligned, with a fixed 1 MiB buffer
  (128 x 8 KiB bounce pages through `blk_ops`); source and destination may be
  raw devices (`blk_open(0/1)`) or an image file on FAT32 (write path via
  `RepairToken`).
- **Safety**: requires `RepairToken` + an explicit `YES`; refuses when the
  destination is smaller than the source; refuses to overwrite the mounted
  source; prints a plan (source/destination/size) before starting.
- **Verification**: SHA-256 (`kernel/src/crypto`) computed while writing and
  re-read from the destination at the end (`verify`; a `--quick` mode hashes
  only the first/last 1 MiB plus a strided sample).
- **Error policy**: read errors abort by default; `--continue` records the
  LBA in a bad-sector list and zero-fills the destination sector. The list is
  printed and offered as a text file on the ESP.
- **UI**: shell command `clone <src> <dst> [--quick|--continue]` with
  progress every 1%; the M15 Qt frontend drives the same operation over IPC.

### 2a. Implemented in M12-2 (2026-09)

The core landed behind **`CONFIG_IMAGER`** (default n; the rescue/net/tls/
desktop/hypervisor profiles enable it). This batch is raw-device-to-raw-device
only; the image-file destination and the `--quick`/`--continue` policies are
M12-3+.

- **Addressing**: `clone <src> <dst> [--verify] [--yes]`, where the paths are
  the block registry's handles `blk0..blk3` (`blk_open(index)`); the C AHCI
  probe now registers **every populated port** (PX_SSTS.DET == 3) in port
  order, so `blk0`/`blk1` are two disks on one controller. The copy is
  sector-based with a 512-byte logical sector, which is what every
  supported transport (AHCI/NVMe/virtio-blk/PIO ATA) reports.
- **Policy** (`kernel-core/src/shell/imager.rs`): print the plan (source/
  destination/name/sectors/KiB and the hashes that will run), enforce the
  hard size gate (`source > destination` is refused before any write), then
  the YES gate ("confirm by typing YES"); `--yes` is for scripted runs.
  `--verify` is accepted but optional to type: verification always runs.
  Only after consent does the command enable repair mode and take a
  `RepairToken` (the type-level gate), mirroring `grub-fix`.
- **Mechanism** (`kernel-core/src/imager/`): 1 MiB bounce buffer (BSS, split
  per chunk; the C drivers bounce per sector internally), pre-copy SHA-256
  of the source, buffered sector copy with progress every 5%, then the
  destination is re-read over the copied range and hashed. Success is
  printed only when the digests match; a mismatch prints both hashes and a
  failure line. Reads and writes retry three times before aborting; errors
  distinguish source-read, mid-copy write, cancel and the first-write case.
- **Read-only destinations**: the first failed `blk_write` (no sector
  written yet) reports `clone: destination is read-only on this build` —
  what the i686 PIO ATA stub (`blk_write` returns -1) produces. i686 has no
  shell yet, so there is no i686 runtime transcript; the rescue/IMAGER i686
  build links the imager and its `blk_open`/`blk_name`/`blk_identity` seams
  (`kernel-i686/src/ata.rs`), and `tools/smoke-imager.sh` asserts the
  message is linked in the tested x86_64 rescue ELF. QEMU does not offer a
  usable read-only IDE backend (it refuses `readonly=on`, and `blkdebug`
  write injection is not traversed on the AHCI path), so the runtime
  first-write failure is deferred to the i686 shell work.
- **Cancellation**: `q` at any chunk boundary aborts and prints that the
  destination is partially written and not verified.
- **Smoke**: `tools/smoke-imager.sh` — one bounded boot with the test disk
  (ESP autorun) plus a small pattern source and two empty destinations;
  asserts the plan/gate transcripts, kernel hashes vs host sha256, the
  verified round trip and the untouched destination tail.
- **Route**: the kernel command is the immediate deliverable because raw
  block devices have no user ABI yet; like the R7 ping/nslookup/wget bridge,
  a userland imager against a later block-device ABI is the cleaner home and
  the hard part (the hash-verified copy engine in `kernel-core::imager`)
  already separates policy from mechanism for that migration (APPS.md).

### 2b. Implemented in M12-3 (2026-09)

The bad-sector policy and the report file landed on top of M12-2. The
default behavior is unchanged: an unreadable sector aborts the run (the
source pre-hash fails before anything is written) with the exact LBA.

- **Flags** (`kernel-core/src/shell/imager.rs`): `clone <src> <dst>
  [--quick] [--continue] [--retries N] [--verify] [--yes]`. `--retries N`
  (1..=16, default 3) sets read retries per sector before it counts as bad;
  the plan print now includes `clone: policy continue=… quick=… retries=…`.
- **`--continue`** (`kernel-core/src/imager/`): a failed chunk read is
  retried `--retries` times, then isolated sector by sector. Every sector
  that stays unreadable is recorded in a fixed-capacity ledger (16 ranges,
  coalesced; totals keep counting past the cap) and zero-filled in the
  output, and the copy carries on. The pre-hash zero-fills the same ranges by
  construction, so the source hash is the "zero-filled expectation"; the copy
  pass never re-reads a recorded sector and hashes exactly the bytes it
  writes (`stream sha256`). Verification re-reads the destination and
  compares it to that stream, and `source-match` records whether the source
  stayed identical between the passes. The verdict is `partial` whenever
  anything was zero-filled (or the source changed) — a `--continue` copy is
  never silently presented as a byte-for-byte copy.
- **`--quick`**: all three hash passes (source, stream, destination) cover
  only the M12 design's sample — the first and last 1 MiB plus 1 MiB at
  25/50/75%, aligned down to 1 MiB. The report marks `verify quick`; the
  verdict wording is unchanged. `--quick` is a verification-cost knob, not a
  substitute for `--continue`.
- **Report** (`kernel-core/src/imager/report.rs`): every clone copy run ends
  with a deterministic report written to **`/tmp/clone-report.txt`** (the P1
  tmpfs; the shell's `cat` does not read tmpfs, so the full report is also
  mirrored to the serial log in one write, prefixed `clone-report:`). Fields:
  source/destination (handle, driver, sectors, bytes), sector size, policy,
  `verify full|quick`, copied sectors, source/stream/destination SHA-256
  (or `none`), `source-match`, one `bad-range lba=… count=… errors=… retries=…`
  per range, `bad-ranges`/`errors`/`retries` totals, and the verdict
  (`verified`/`partial`/`failed`/`cancelled`). The format is stable and
  greppable by the smoke.
- **I/O recovery fix** (`drivers/c/ahci_io.c`): a failed command leaves
  `PxCI` set (QEMU keeps the slot busy until software clears it; real HBAs
  can on timeout), which wedged the port and made every retry burn the full
  2 s poll. The command path now watches `PxIS.TFES` for the error, kicks
  the port (stop the command engine, clear the latched IRQ/SERR status,
  restart) after a failure and retries cleanly. Without this, `--continue`
  could neither retry nor read the sectors after a bad one.
- **Fault injection** (`tools/mkdisk.py --badclusters`,
  `tools/mkimagerdisks.sh`): the fixture writes a deterministic pattern and
  an `<image>.bad` sidecar listing `lba count` ranges. `tools/run.sh
  --imager-bad` turns each sector into one QEMU **`blkdebug` `inject-error`**
  rule (`event = "read_aio"`, `sector = <lba>` in 512-byte sectors,
  `errno = 5`), wrapping the source drive. blkdebug is a block-layer filter,
  so the   guest observes a real AHCI read error through `blk_read` — no guest
  output is faked and no test hook is compiled into the kernel. (The M12-2
  note stands for writes: write injection was not usable on this path and
  is not used; the smoke only needs source read errors.)
- **Smoke** (`tools/smoke-imager-bad.sh`): one bounded boot that runs three
  clones from the ESP autorun — `--quick` happy path, default abort at LBA
  100, then `--continue`. It asserts the abort transcript, the zero-fill
  lines, the full report (ranges/counts/hashes/`verdict partial`), the quick
  report, and the host-side zero-filled destination prefix (`sha256` of the
  pattern with the injected ranges zeroed). `tools/smoke-imager.sh` is
  unchanged and stays green.

## 3. NTFS read-only

- **Scope (v1)**: boot sector/BPB, `$MFT` parse, FILE records (resident and
  non-resident attributes), runlist decoding, directory index
  (`$I30`) lookup/listing, `$DATA` streaming reads for files up to the
  reader's buffer. No compression, no encryption (EFS), no sparse-file
  materialisation beyond zero-fill, **never a write** — the driver has no
  write entry point at all.
- **Integration**: a new `vfs/ntfs/` module (split per the line rule) with
  the same shape as ext4: `Ntfs::parse(part_lba)`, `describe`, `list`,
  `read(path)`. Mounted read-only at `/mnt/win0` when a Windows partition is
  found. The probe table already detects NTFS; it graduates from
  "probe-only" to "mounted ro".
- **Limits**: path lookup is 8.3-agnostic (UTF-16 names decoded to a fixed
  buffer, truncated safely); very fragmented files are fine (runlist walk),
  but attribute lists spanning multiple MFT records are a v2 candidate.
- **Verification**: a `tools/mkdisk.py --ntfs` fixture (hand-built minimal
  NTFS with one directory and two files), plus a real Windows disk image
  smoke; listing must match `ntfsls` output for the fixture.

### 3a. Implemented in M12-4/M12-5 (2026-09)

The read-only reader landed behind **`CONFIG_NTFS`** (default n; the
rescue/net/tls/desktop/hypervisor profiles enable it) and the minimal
kernel carries no NTFS code or `/mnt/win0` string.

- **Reader** (`kernel-core/src/vfs/ntfs/`, split per the line rule):
  `mod.rs` validates the boot sector (OEM `NTFS    `, 512-byte sectors,
  power-of-two clusters up to 4 KiB, MFT LCN/record size and cluster
  count) and bootstraps `$MFT` from record 0's `$DATA` runlist (fragmented
  runs included); `record.rs` parses FILE records with update-sequence
  fixups and the attribute list (`$STANDARD_INFORMATION` attributes,
  resident/non-resident `$DATA`); `runlist.rs` decodes packed runs
  (fragmented and sparse, <= 24 runs) and `file.rs` streams `$DATA`
  through them, zero-filling sparse runs and anything past the initialized
  size; `index.rs` walks `$I30` from the resident INDEX_ROOT through the
  INDEX_ALLOCATION B-tree (breadth-first, bounded queue, INDX fixups);
  `cache.rs` is the bounded read cache (two 4 KiB cluster slots, an
  `IrqLock` around slot access, the device read unlocked). Compression,
  encryption and attribute lists are detected and rejected with a clear
  error (`NtfsErr::text()`); attribute lists that would span MFT records
  are the documented v2 item.
- **Mount and tools**: `vfs::init` mounts the first parseable NTFS volume
  read-only at `/mnt/win0` (`vfs/ntfs/api.rs` holds the shared mount), the
  probe label graduates to `NTFS (mounted ro)`, `lsmnt` lists it and the
  new `ls [path]` command lists NTFS/FAT/ext4 directories. `cat
  /mnt/win0/...` dumps up to 4 KiB and prints the full-file SHA-256 (the
  smoke compares it to the host fixture). The POSIX fd layer
  (`vfs/ntfs_fd.rs`) serves read/readdir/stat under `/mnt/win0` so
  userland `ls`/`cat` work under `sh`/`bash`; every write intent
  (`open` with O_WRONLY/O_RDWR/O_TRUNC/O_APPEND/O_CREAT, unlink, mkdir,
  rename) returns `EROFS` ("Read-only file system") before any filesystem
  code runs. The NTFS reader itself has **no write entry point at all**.
- **Fixture** (`tools/mkntfs.py` + `tools/mkntfs_fs.py`): the host has
  `mkfs.ntfs`/ntfs-3g (mkntfs v2026.7.7), but the smoke must stay
  deterministic and host-tool-free, so the fixture is a hand-built 16 MiB
  NTFS 3.1 volume (2048 4 KiB clusters, 1024-byte MFT records): `$MFT`
  with `$MFTMirr`/`$LogFile`/`$Volume`/`$AttrDef`/`$Bitmap`/`$Boot`/
  `$BadClus`/`$Secure`/`$UpCase`/`$Extend`, a root directory indexed
  through `$I30` + one INDX block, a small resident-index `Users`
  directory, resident files (`hello.txt`, `résumé.txt` with a UTF-16
  non-ASCII name, `Users/alice.txt`, `Users/logs/boot.log`), a
  non-resident **three-run fragmented** file (`frag.bin`, 10000 bytes) and
  one FILE record with a deliberately broken update sequence
  (`corrupt.txt`). Every byte is deterministic; the image is validated
  against ntfs-3g (`ntfsls`, `ntfscat`, `ntfsinfo`) at development time.
  `mkdisk.py --ntfs` places it on the delivered GPT disk (partition 2, or
  partition 4 after `--two-fs`) and installs the ESP autorun transcript.
- **Smoke** (`tools/smoke-ntfs.sh`): one bounded boot asserts the mount
  line and volume facts (`4096 clusters of 4096 B, MFT record 1024 B at
  LCN 4`), root/`Users` listing equality, the kernel SHA-256 of the
  resident, fragmented and non-ASCII files against `sha256sum` of the
  extracted fixture bytes, the corrupt-record rejection and the missing
  path error, the userland `sh -c` transcript including
  `Read-only file system`, and that the rebuilt image is byte-identical
  after the run (`cmp`) — a write could not have reached the volume.
- **Limits/out of scope (v1)**: compression, encryption (EFS), sparse-file
  materialisation (sparse runs read as zeroes), `$ATTRIBUTE_LIST`
  continuation records, `$Bitmap`/`$LogFile` semantics (they are present
  and parsed as ordinary metadata, never interpreted), alternate data
  streams, disk quotas/reparse points, and any write path. 512-byte
  sectors, clusters <= 4 KiB, MFT records <= 1 KiB and runlists <= 24 runs
  are the validated geometry. The real Windows image check stays optional:
  this host has no Windows 10 image, so the Win10
  `Windows/System32` listing is an OPERATIONS manual/CI-optional path.

## 4. AMD GPU probe (report-only)

Measured signals only; no vendor-table guesswork without a documentation
spike:

| Signal | Source | Notes |
|---|---|---|
| Identity | PCI vendor/device/class | AMD vendor 0x1002; display class 0x03 |
| BARs | PCI config | sizes via the write-1s probe; the VRAM BAR is mapped **read-only** |
| PCIe link | PCIe capability registers | current + max speed/width; decode generations |
| Thermal | ACPI thermal zone (requires the ACPI table parser, below) | report only when exposed |
| VRAM reachability | conservative reads within the mapped BAR | read a bounded pattern range twice; no writes |

Prerequisite: a minimal **ACPI table walker** (RSDT/XSDT -> FADT/DSDT lookup
for thermal zones and IOMMU tables); it is small and also feeds M12's
virtualization detection (DMAR/IVRS). VBIOS/ATOM firmware interpretation is
explicitly deferred until the M12 spike collects vendor documentation.

### 4a. Implemented in M12-6 (2026-09, report-only; QEMU-only acceptance)

The probe landed behind the existing **`CONFIG_GRAPHICS`** gate (shared with
the M5.5 GPU presence check and the C4 gating), and the `rescue` profile now
selects `GRAPHICS` so the rescue console carries the report. The boot stage
prints the block and the x86 rescue table registers a `gpu` command that
re-prints it; the minimal/net profiles link neither.

- **Mechanics** (`kernel/src/arch/x86_64/pci_probe.rs`, `kernel/src/diag/
  gpu.rs`): display-class devices (base class 0x03) are enumerated by the
  existing `pci.rs` catalog. Config space only: subsystem IDs (0x2C), the
  capability-list walk (bounded to 48 hops, alignment/cycle-checked), the
  standard write-1s BAR sizing probe (the original value is restored
  immediately; no MMIO register is written), and the PCIe capability
  (Link Capabilities at +0x0C, Link Status at +0x12). Each memory BAR up to
  256 MiB is mapped through the `PHYS_OFFSET` window (`map_mmio`, 2 MiB
  pages, PCD/PWT; interrupts off across the page-table update) and its first
  dword is read once as a reachability probe. Larger VRAM BARs are reported
  unmapped — no walk, no write, no unbounded loop.
- **Report lines** (stable, greppable; two-space diag-stage indent):
  `gpu: 00:02.0 1234:1111 QEMU stdvga [display] ss=1af4:1100`,
  `gpu: bar0 0x80000000 size 16M (mapped ro)` (or
  `(mapped ro, 64-bit)` / `(not mapped[, 64-bit])`),
  `gpu: pcie link 2.5GT/s x1` (or `(max …)` when the maximum differs, or
  `gpu: pcie n/a (no PCIe capability)`),
  `gpu: pci display devices found: N`, and
  `gpu: thermal unavailable (no ACPI TZ)` /
  `gpu: thermal zone present (ACPI _TZ_; temperature read not implemented)`.
  Known IDs: QEMU stdvga (0x1234:0x1111), Cirrus GD5446 (0x1013:0x00B8),
  virtio-gpu, QXL, and the AMD RX 500/5000/6000 families plus common iGPUs;
  unknown IDs still print the raw vendor:device and the vendor name.
- **ACPI thermal hook** (`kernel/src/acpi.rs`): the FADT is parsed for the
  DSDT (offset 40, or X_DSDT at 140 on ACPI 2.0+), the DSDT header is
  checksum-verified and clamped to 1 MiB, and the image is scanned for the
  `_TZ_` thermal-zone name. This is a hook-presence signal only — no AML is
  interpreted and no temperature is read; the ACPI summary line gained
  `dsdt=… tz=…`.
- **Acceptance (QEMU-only, accepted by the owner)**: `tools/smoke-gpu.sh`
  boots `-vga std` (0x1234:0x1111, BAR0 16 MiB mapped ro), `-vga cirrus`
  (0x1013:0x00B8, BAR0 32 MiB) and `-vga virtio` (a 64-bit MMIO BAR above
  4 GiB mapped ro) on the rescue profile, asserting the identity/BAR lines
  from both the boot block and the shell `gpu` command plus the
  `pcie n/a`/no-TZ lines. `tools/smoke.sh`'s main and shell phases assert
  the std block; `tools/smoke-config.sh` asserts the minimal ELF has no
  `gpu` token while the rescue profile links it.
- **Deferred to real hardware (manual follow-up)**: QEMU display models
  expose no PCIe capability (checked on i440fx and q35), so the `pcie link`
  decode and the DSDT `_TZ_`-present branch are exercised only as code
  paths here. On one real AMD RX 500/6000 GPU the operator records the
  `pcie link` line and the thermal line (see OPERATIONS §5.1); the identity
  line and BAR sizes come from the same probe. VBIOS/ATOM interpretation
  and any temperature evaluation stay out of scope.

## 5. Virtualization V1 detection

- CPU: `CPUID.1:ECX[31]` hypervisor present; `CPUID.40000000h` vendor
  string (KVM/Xen/VMware/Hyper-V).
- Intel: `CPUID.1:ECX[5]` VMX; `IA32_FEATURE_CONTROL` MSR; EPT/VPID
  capability MSR (`IA32_VMX_EPT_VPID_CAP`); VT-d via the ACPI DMAR table.
- AMD: `CPUID.80000001h:ECX[2]` SVM; `CPUID.8000000Ah:EDX[0]` NPT; AMD-Vi
  via the ACPI IVRS table.
- Output: a one-line verdict plus the compatibility matrix rows from
  `ROADMAP_v0.0.2+.md` §9, and the fallback policy (`physical mount`) when
  hardware virtualization is absent — consumed by the M15 disk-service VM.

## 6. Work breakdown (suggested commits)

| Step | Deliverable |
|---|---|
| M12-1 | ACPI table walker (RSDT/XSDT/RSDP, table lookup helper) |
| M12-2 | Disk imager core + `clone` shell command + hash verification |
| M12-3 | bad-sector policy (`--continue`) + report file |
| M12-4 | NTFS boot/MFT/attribute/runlist read path on the fixture |
| M12-5 | NTFS directory listing + `read` + `/mnt/win0` mount, probe graduation |
| M12-6 | AMD/PCI GPU report (identity/BAR/link) |
| M12-7 | Virtualization detection + matrix + docs; smoke phases; THIRD_PARTY clean |

## 7. Verification

- Imager: image-to-image and disk-to-image round trips hashed
  (`tools/smoke-imager.sh`); a crafted bad-sector fixture (mkdisk
  `--badclusters` + QEMU blkdebug read errors) exercises `--continue`, the
  zero-fill expectation and the report (`tools/smoke-imager-bad.sh`).
- NTFS: fixture listing/read equality; a real Windows 10 image lists
  `Windows/System32` entries (read-only).
- GPU: `tools/smoke-gpu.sh` boots QEMU std/cirrus/virtio and asserts the
  identity, known-ID name, BAR sizes and the mapped aperture (QEMU exposes
  no PCIe capability and no ACPI thermal zone, so those report as `pcie
  n/a` / `thermal unavailable`, which is asserted). On one real AMD GPU
  (RX 500/6000 series) the link/thermal report is recorded manually —
  OPERATIONS §5.1; the QEMU-only acceptance of M12-6 is an owner decision.
- Virtualization: on a bare host (matrix rows for the local CPU), inside
  KVM/Xen guests (hypervisor vendor string), and on a pre-2010 VM without
  EPT/NPT (fallback wording).
- All earlier smokes stay green on x86_64/i686 and riscv64/arm64.

## 8. Risks

| Risk | Mitigation |
|---|---|
| NTFS scope creep (compression, attribute lists) | v1 list above is fixed; v2 items documented |
| GPU probing hangs fragile hardware | read-only BAR maps, bounded reads, timeouts, report-only |
| Imager silently corrupts a destination | mandatory plan print, size check, hash verify, YES gate; `--continue` is never silent: every zero-filled range is counted in the report and the verdict becomes `partial` |
| ACPI parser bugs | minimal table walker, checksum-verified, never trusts lengths |
| Virtualization claims overreach | matrix states exactly what was detected; isolation claims deferred to M15's threat model |

## 9. Shipped (v0.0.4)

The four M12 workstreams shipped and are verified by their gates:
**W-a** (disk imager + `clone`, `tools/smoke-imager.sh`), **W-b** (bad-sector
policy + report, `tools/smoke-imager-bad.sh`), **W-c** (read-only NTFS with
`/mnt/win0`, `tools/smoke-ntfs.sh`) and **W-d** (report-only GPU/PCI probe,
`tools/smoke-gpu.sh`). M12-6 shipped with **QEMU-only acceptance** (an owner
decision): QEMU display models expose no PCIe capability and its DSDT has no
thermal zone, so the real AMD RX 500/6000 `pcie link`/thermal capture stays
the OPERATIONS §5.1 manual follow-up. The i686 read-only imager branch is
documented rather than runtime-tested — i686 has no shell, so there is no
interactive `clone` transcript there (see §2a).
