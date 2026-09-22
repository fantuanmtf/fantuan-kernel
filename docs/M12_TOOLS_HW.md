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
- GPU: on QEMU (boot VGA/cirrus) identity+BARs only; on one real AMD GPU
  (RX 500/6000 series) the link/thermal report is recorded.
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
