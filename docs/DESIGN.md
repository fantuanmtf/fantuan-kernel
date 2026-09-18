# fantuan-kernel — Design Document

> Status: v1 living draft. Change this document before changing code.
> Language policy: **all repository artifacts — code, comments, commit messages,
> documentation, kernel messages — are written in English.** (Design discussions may
> happen in any language; the repo itself is English-only.)

## 1. Vision

fantuan-kernel is a self-written operating system kernel targeting **live rescue systems**.
It boots from removable media on real machines, diagnoses hardware and boot-chain problems,
and repairs broken boot setups — primarily Linux, with read-only diagnosis for BSD.
Windows repair is explicitly deferred.

Design drivers:

- **Offline-first** — no network drivers in v1.
- **Read-only-first** — a rescue kernel must never write to a disk the user did not
  explicitly ask to modify. Every automatic action is read-only.
- **Evolvable** — interfaces are append-only and versioned; features can be added or
  removed without breaking the core.

## 2. Language / Layer Split

| Layer | Language | Rationale |
|---|---|---|
| EFI bootloader | Rust (`x86_64-unknown-uefi`) | UEFI apps are PE32+; EFI calls use the UEFI calling convention; writing this layer in assembly is a dead end. |
| Kernel entry stub, context switch, interrupt entry, AP trampolines | Assembly | Permanent by nature — the "forever assembly". |
| Kernel core (mm, scheduler, VFS, syscall, diagnostics) | Rust | Memory safety where most kernel bugs live. |
| Device drivers (AHCI, NVMe, xHCI, USB, future NICs) | C | Decades of C driver heritage; NetBSD / rump-kernel stack is the intended migration source. |
| Userspace (future) | C/Rust over own ABI + own libc | Incremental, POSIX-shaped. |

### 2.1 What "drivers are C" does *not* mean

The Rust core owns **minimal hardware access** required before any driver can load:
CPUID, MSRs, port I/O, MMIO, and **PCI configuration-space enumeration**. These are
arch code, not drivers. Stage-1 diagnostics run on them with zero dependency on the C
driver layer. "C drivers" means real device interaction: bus transactions, IRQs, DMA.

## 3. Encoding Policy

Firmware and peripherals disagree on text encoding. Normalize at every boundary
instead of fighting each firmware:

| Boundary | Reality | Policy |
|---|---|---|
| UEFI firmware | Strings are UTF-16 (`CHAR16`) | EFI bootloader converts to UTF-8 before handoff. |
| Kernel internal | Rust `str` is UTF-8 | Canonical encoding = UTF-8; in practice v1 uses the ASCII subset (all messages are English). |
| Console | GOP framebuffer + embedded 8x16 bitmap font | v1 renders ASCII only (plus CP437 box-drawing if cheap). No CJK rendering in the kernel console, ever. |
| Serial | 16550 UART | 8N1, ASCII. |
| SMBIOS | OEM encodings vary | Treat string tables as raw bytes; sanitize to printable ASCII for display. |
| ACPI | Binary tables; 4-char ASCII names; AML bytecode | No text encoding issues. |
| Beep codes | None | Encoding-independent by design; the fallback channel on machines with no usable display. |

## 4. Boot Chain

```
UEFI firmware
  -> EFI app (Rust): locate kernel, set GOP mode, capture RSDP + memory map,
     ExitBootServices (retry on map-key change), build BootInfo
  -> build initial page tables (identity 4 GiB + PHYS_OFFSET alias, 2 MiB pages),
     enable paging, jump to the higher-half kernel entry
  -> assembly entry stub: reload GDT/IDT, set kernel stack, jump to kmain
  -> kmain (Rust)
```

- **v1 kernel image delivery**: the kernel (flat binary, linked at 0x1000000 —
  16 MiB) is read from `\fantuan\kernel.bin` on the ESP via the firmware's Simple
  File System Protocol (Open/Read/GetInfo). No FAT parsing lives in our code — the
  firmware does it — and no PE-section embedding or objcopy tricks are needed.
  (Amended during M0: SFS file loading is strictly simpler and more robust than
  section embedding; the link address moved from 1 MiB to 16 MiB because low
  memory is a firmware minefield — OVMF placed the EFI app image itself right
  above 1 MiB, inside the kernel's .bss footprint. The bootloader allocates the
  initial kernel stack below the kernel image so the firmware can never hand out
  pages inside it.)
- **Handshake verification at every stage boundary**: each stage validates the magic
  and version of what the previous stage handed over. Boot log chain:
  ConOut (pre-exit) -> serial (post-exit) -> GOP console.
- **Boot errors are codes, not hangs**: any boot failure is a human-readable error
  code plus stage name, never a silent black screen.
- **Known pitfalls encoded in the bootloader**: ExitBootServices may return
  `EFI_INVALID_PARAMETER` (memory map changed — must re-GetMemoryMap and retry);
  after exit, Boot Services are dead (kernel must own its output from instruction
  one); UEFI runs under identity mapping (entry stub must load its own page tables
  before touching CR3); Runtime Services use physical addresses pre-
  `SetVirtualAddressMap`.

### 4.1 BootInfo v1 (bootloader <-> kernel ABI)

```rust
#[repr(C)]
pub struct BootInfo {
    pub magic: u32,        // 0x4654_4E46 "FTNF"
    pub version: u32,      // 1; kernel rejects unknown versions
    pub memmap: MemMap,    // EFI memory map (physical ranges + types)
    pub fb: FrameBuffer,   // GOP linear framebuffer (base, w, h, stride, format)
    pub rsdp: u64,         // ACPI RSDP physical address
    pub kernel_base: u64,  // where the kernel was loaded
    pub stack_top: u64,    // bootloader-allocated initial kernel stack (added in M0)
    pub caps: u64,         // capability bits, append-only
    pub boot_pml4: u64,        // bootloader-built initial PML4, physical (M2)
    pub boot_tables_pages: u64, // 4K pages those tables occupy (M2)
}
```

Versioned, append-only; capability bits are the negotiation mechanism so features
can be added or removed without breaking old components.

## 4.5 Memory Model (v1)

- **Layout convention**: every physical address `p` is reachable at
  `PHYS_OFFSET + p` with `PHYS_OFFSET = 0xFFFF_8000_0000_0000` (-2 GiB,
  Linux-style; defined once in the `fantuan-abi` crate). The first 4 GiB are
  additionally identity-mapped during M2 so pre-existing physical pointers
  (framebuffer, EFI structures) keep working; the identity map is a bootstrap
  convenience and may be dropped once all physical pointers are migrated.
- **Kernel image**: linked at `PHYS_OFFSET + 16 MiB`; the bootloader loads the
  flat binary at physical 16 MiB and jumps to the higher-half entry. kmain
  asserts its own address at boot.
- **Initial page tables**: built by the bootloader (identity 4 GiB + PHYS_OFFSET
  alias, 2 MiB huge pages, 11 pages, allocated below the kernel image). The
  kernel immediately rebuilds an equivalent set from its own frame allocator,
  switches CR3, and reclaims the bootloader's tables.
- **Frame allocator**: bitmap over the EFI memory map; reclaims LoaderCode/Data,
  BootServices Code/Data, Conventional, and Unaccepted (type 15) memory; punches
  holes for the kernel image, boot stack, BootInfo, memory-map buffer and the
  bootloader page tables. Never hands out frames below 1 MiB. Covers the first
  4 GiB (M2 scope; extended when RAM > 4 GiB matters).
- **Build**: the kernel uses `-C code-model=large` (higher-half addresses do
  not fit the small model's 32-bit relocations). Page-table entries always hold
  PHYSICAL addresses; virtual pointers are only for writing them. The large code
  model renames sections (.ltext/.ldata/.lbss), so the linker script matches
  both families.

## 4.6 Scheduler & Syscall ABI (v1)

- **Tasks (M3)**: kernel-mode tasks on frame-allocated 16 KiB kernel stacks;
  static table (16 slots). Round-robin with a 100 ms quantum driven from the
  IRQ0 handler; `switch_context` (assembly, DESIGN.md §2) saves/restores the
  callee-saved registers on the task stacks, so the switch unwinds inside the
  next task's own interrupt frame. Sleeping tasks wake by tick deadline.
  M3 scope notes: no ring 3 yet, exited tasks leak their stacks (reaping in
  M4), the frame allocator is reachable via a boot-time raw pointer (becomes a
  proper global in M4).
- **Syscall ABI v1**: INT 0x60 gate (DPL 3 since M4); rax = number,
  rdi..r8 = five arguments, rax = result; 0 = OK, u64::MAX = ENOSYS,
  u64::MAX-1 = EINVAL. Versioned ABI: `SYS_VERSION` (0) probes `ABI_VERSION`
  and the call numbers live in `fantuan-abi`, which both sides include — new
  calls are appended, never renumbered. v1 carries no per-call version
  argument (calls are additive and capability-gated); a per-call version
  word is kept as an M8 ABI-hardening option if a call ever needs a breaking
  semantic change. Calls: exit(1), sleep_ms(2),
  write(3) (kernel debug channel), get_tid(4), yield(5). The trampoline
  (assembly) reserves scratch below rsp so the interrupt frame never touches
  the caller's red zone.
- **User mode (M4)**: ring 3 via a pre-built iretq frame ([rip][cs=0x33]
  [rflags=0x202][rsp][ss=0x2B]) entered through the `user_entry` assembly
  stub. Per-task page tables: user PML4 clones ONLY the kernel half
  (PHYS_OFFSET alias) — the identity map stays out so the user half is free
  for 4K mappings. The scheduler sets TSS.rsp0 to the next task's kernel
  stack top BEFORE the switch (a fresh user task iretqs immediately and never
  resumes schedule()). ELF loader: static ET_EXEC, PT_LOAD, 4K pages, shared
  pages between segments are reused, mid-page segment starts supported.
  User faults kill the task (classified from the frame's CS). M4 notes:
  user pages are RWX, no SMEP/SMAP, exited tasks leak stacks + page tables,
  the serial lock excludes interrupt-context writers by design.

## 5. Rust <-> C FFI Boundary

- Rust core exports a narrow C interface, **rust_core.h** (v1 implemented in M4.5,
  `kernel/src/drivers.rs`):
  `k_log`, `k_log_hex`, `k_phys_to_virt`, `k_alloc_page` (single 4K DMA
  page, returns virt + physical out-param), `k_delay_ms`. Grows conservatively:
  each addition is a deliberate, documented widening of the boundary.
- C drivers expose **ops tables** (`drivers/c/include/driver.h`); v1 is the
  block-device read path only:
  ```c
  int blk_read(void *dev, uint64_t lba, void *buf, size_t sectors); // polling, ro
  ```
  Writes, IRQ registration and the full ops table arrive when the first
  writable filesystem needs them (M6).
- Ownership contracts are written into the header comments (who allocates, who frees,
  DMA buffer rules). Build integration: `build.rs` + `cc` crate with
  `-mcmodel=large -mno-red-zone -ffreestanding`; C code lives in
  `drivers/c/`, statically linked.
- **PCI enumeration lives in the Rust core** (`kernel/src/pci.rs`, DESIGN.md
  §2.1: minimal hardware access) and hands the ABAR to the C probe — the C
  layer never touches PCI config space itself.
- **Storage device abstraction** (`open_dev / capacity / read_lba`) — never
  SATA-specific loops. **Implemented**: `drivers/c/blk.c` owns a device
  registry and dispatches the driver-agnostic ops (`blk_read`, `blk_write`,
  `blk_identity`, `blk_smart_read_data`, `blk_smart_read_log`); each driver
  registers a `struct blk_ops` after probing. `drivers/c/ahci.c` (M4.5) and
  `drivers/c/nvme.c` (M8) both register the same table, so the VFS, boot
  repair and diagnostics never learn which transport served the bytes.
  A NULL handle means "the first registered drive".
- **NVMe** (M8): polling driver — controller reset, admin queue, Identify
  Controller/Namespace, one I/O queue pair, PRP1 transfers through a 4 KiB
  page (8 sectors per command), SMART/Health log page 0x02. NVMe BARs are
  64-bit and sit above 4 GiB, so the kernel grew
  `mm::paging::map_mmio()`: it builds the missing PML4/PDPT/PD levels on
  demand and maps the register window into the PHYS_OFFSET alias with
  PCD|PWT (2 MiB pages) — the same path a real laptop needs.
- Known trade-off: C drivers can corrupt the kernel. Accepted for v1; long-term
  option is moving drivers into isolated userspace processes.

## 6. Diagnostics Framework

- Interface: `trait Diagnostic { fn name() -> &'static str; fn quick_check() -> Report;
  fn deep_check() -> Report; }` — each check is one implementation; add/remove freely.
- Severity levels: `OK | Warning | Critical`. Every diagnostic is **read-only**.
- Two stages, matching boot order:
  - **Stage 1 (pure Rust core, seconds)**: CPU -> GPU presence -> RAM quick test.
  - **Stage 2 (after C drivers load)**: storage scan -> SMART summary -> OS
    identification -> auto ro-mount.
- **v1 implemented (M5 + M5.5, `kernel/src/diag/`)**: the framework (Check +
  Severity + per-stage runner) and the checks — cpu (SMBIOS identity lines +
  CPUID brand/topology/features/µcode), gpu (PCI display catalog + §6.2 beep
  codes incl. the slot-vs-PCI "2 short" dGPU check), ram (pattern test over
  allocator-borrowed frames), storage in the order ① IDENTIFY strings →
  ② SMART health line → ③ per-partition filesystem types → ④ ESP bootloaders.
  SMBIOS discovery prefers the UEFI configuration table (BootInfo.smbios_table)
  with an F-segment anchor scan as fallback; absent tables degrade gracefully.
  Surface scan (shell `diskhealth --scan`), NVMe SMART (M8) and the storage
  ops registry have since landed; the remaining v1 diagnostic gap is UFS
  (diagnosis-only, §9).

### 6.1 Stage 1 checks

- **CPU**: CPUID brand/family/model, topology (SMT), feature bits (NX, SMEP/SMAP,
  AVX), microcode revision (`IA32_BIOS_SIGN_ID`), temperature/throttling MSRs
  (read-only). Topology data feeds the scheduler.
- **GPU — presence check only** (no rendering tests; console is GOP):
  PCI enumeration for class 0x03, vendor/device ID -> small built-in name table, VRAM
  BAR size. Missing-but-expected dGPU => report probable slot/power/firmware fault.
- **RAM quick test**: only Conventional Memory from the EFI map (never ACPI/runtime/
  reserved). Multi-core sharding, 0xAA/0x55/walking patterns. Runs before the
  scheduler exists (the memory under test must be untouched). Deep own-address test:
  optional, background, later. SMBIOS Type 17 (capacity/ECC) reported alongside.
  BIOS POST already covers hard failures; this layer catches marginal bits — the
  "boots but randomly crashes" class.

### 6.2 Beep codes

Beeps use the PC speaker (PIT channel 2 + port 0x61 gate) — available in stage 1,
no driver dependency. Every code is also written to serial/console logs (sound is an
auxiliary channel, never the only one).

| Code | Condition | Meaning |
|---|---|---|
| 2 short | SMBIOS Type 9 slot "In Use" but no dGPU enumerated in PCI | dGPU plugged in but not detected (slot/power/firmware) |
| 1 short | No iGPU device present | No integrated GPU |
| 4 short | Neither iGPU nor dGPU detected | No display device at all — supersedes 2+1 (no ambiguous 7-beep sequence) |

Boot-complete signal: **one LONG beep** — deliberately distinct from the short-beep
diagnostic codes above, so a healthy boot can never be mistaken for a GPU fault.

## 7. Disk Health (SMART, read-only)

- **SATA (ATA commands)**: power-on hours (attr 0x09); bad-block signal: reallocated
  (0x05), pending (0xC5), uncorrectable (0xC6); host reads/writes from the
  standardized Device Statistics log, SMART attrs 0xF1/0xF2 as vendor fallback.
- **NVMe (log page 0x02)**: Power-On Hours, Data Units Read/Written (value x 1000 x
  512 bytes), Percentage Used, Media Errors.
- **Surface scan**: optional (`diskhealth --scan`), background, progress bar,
  cancellable; reports slow sectors (>500 ms) and read errors. **Off by default** —
  full scans can accelerate the death of a failing drive; rescue-first.
- All quantities displayed in decimal GB/TB (matching vendor marketing units).
- Example report:

```
nvme0n1  SSD   power-on 3217 h (134 d)   read 3.2 TB   written 5.1 TB   life 92%
sda      HDD   power-on 28640 h   reallocated 0   pending 0   uncorrectable 0   [not scanned]
sdb      HDD   power-on 60211 h   reallocated 12   pending 3   <- WARNING: back up first
```

## 8. Storage Scan & OS Identification

1. Enumerate storage devices via the storage abstraction; read LBA0/LBA1 for MBR
   (`0x55AA`) / GPT (`"EFI PART"`); report model/serial/capacity/boot type.
2. **Probing != mounting**: identify filesystems by superblock magic in the first
   few KB of each partition (ext4 `0xEF53`, XFS `"XFSB"`, Btrfs, FAT, NTFS
   `"NTFS    "`, UFS, swap). ext4 has a real read-only driver since M6.5;
   XFS/Btrfs/NTFS/swap stay magic-probed only (no driver needed for v1).
3. Read `\EFI\` of the ESP to list bootloaders; identify Linux by
   `/boot/grub/grub.cfg`, `vmlinuz`, `initramfs`; identify Windows partitions
   by GPT partition-type GUIDs (works with **zero** NTFS driver).
4. Auto-mount, **read-only**: ESP (FAT32) + identified roots at `/mnt/esp0`,
   `/mnt/root0`. The first parseable ext4 root is mounted ro since M6.5
   (read-only by construction: no journal replay, no writes); FAT32 remains
   the ESP transport. Mount failure degrades to "not mounted + reason" —
   never writes.

Example identification table:

```
nvme0n1p3   ext4      Linux (Debian/Ubuntu style)   -> mounted ro
nvme0n1p1   FAT32     EFI System Partition          -> mounted ro
nvme0n1p4   unknown   Windows data (by GPT GUID)    -> not mounted
```

## 9. Boot Repair v1 Scope

- **Linux (full path)**: diagnose GRUB/systemd-boot/rEFInd/EFI-stub; write ESP files
  (safe, no user data); regenerate `grub.cfg` with a native parser; write NVRAM
  entries via Runtime `SetVariable`; native `grub-install` equivalent.
- **BSD (diagnosis only)**: UFS read-only inspection. ZFS pools => explicit
  "ZFS boot repair not supported" report.
- **Windows**: deferred entirely (closed source). NTFS driver deferred as well;
  GUID-based identification still works without it.
- **UEFI/BIOS settings health check** (report only in v1): Secure Boot state (RT
  `GetVariable`), SATA mode = RAID/RST, stale NVRAM boot entries, CSM/legacy
  mismatch, firmware version (SMBIOS).
- **Implemented so far**: ESP scan + bootloader identification, grub.cfg/fstab
  parsing with UUID cross-checks (M7) — the real `/etc/fstab` comes from the
  ext4 root since M6.5, with the ESP copy as fallback — NVRAM diagnosis via
  Runtime Services —
  BootCurrent self-test, Secure Boot + SetupMode, BootOrder + Boot#### list
  with stale-entry checks including the firmware-default fallback path, and the
  firmware clock (M7.5a). FAT32 repair writes (M7.5b): explicit repair mode
  gates every write, data-before-FAT-before-dir-entry ordering, and the first
  real repair action — copying EFI/ubuntu/shimx64.efi into a missing
  EFI/BOOT/BOOTX64.EFI (verified with a --broken test disk). NVRAM repair
  (M7.6): SetVariable-based repair — BootOrder rebuild, stale-entry deletion
  and boot-entry recreation (an entry "targets the ESP" iff its HD
  device-path node's GPT signature equals the mounted partition's unique
  GUID). Firmware behavior (verified against edk2 + OVMF): once the
  variable policy locks at ReadyToBoot, SetVariable accepts only the boot
  variables (BootOrder / Boot#### / ...) — arbitrary new names get
  EFI_INVALID_PARAMETER, so repair is deliberately scoped to boot entries.
  QEMU's writable pflash allows runtime NV writes on both the SMM and
  non-SMM OVMF builds; the test environment still uses QEMU q35 + SMM OVMF
  (tools/run.sh --smm) because real firmware locks the flash at runtime
  without SMM. The SMM build is SB-enabled and is paired with the plain
  (keyless) vars template to keep Secure Boot off. Config regeneration
  (M7.9, §9.1): `grub-fix install` backs up and rewrites the vendor
  configuration from the mounted ext4 root, then verifies the file and the
  rest of the chain (ESP payloads + NVRAM entry).
- **Architecture note**: classic boot repair uses `chroot` into the target system —
  impossible without a Linux ABI. v1 covers ~80% with native repair; a long-term
  **linuxulator compatibility layer** (FreeBSD-style) is kept as an open option to
  reach chroot/dracut-class repairs. The ABI design must not block this.

### 9.1 Repair write contract (v2, M7.9)

Every repair write is gated behind repair mode plus the operator's explicit
YES (`grub-fix repair` / `grub-fix install`); the `RepairToken` type makes
the gate a compile-time requirement. The v2 contract:

- **Config regeneration** (`grub-fix install`): the target is the detected
  vendor configuration (`EFI/ubuntu/grub.cfg`, else `EFI/debian/grub.cfg`,
  else `EFI/fantuan/grub.cfg`, created if necessary).
- **Backup first**: the existing target is copied to `GRUBCFG.BAK` in the
  same directory and verified by re-reading. If the backup cannot be
  written and verified, the overwrite is aborted.
- **Deterministic generation**: the new configuration comes from the mounted
  ext4 root — `/boot/vmlinuz*`, `/boot/initrd*`, `/etc/os-release`,
  `/etc/default/grub` — with `search --fs-uuid` and the ext4 UUID; no shell
  evaluation, fixed entry order.
- **Verify after write**: the published file is re-read and compared with
  the generated bytes; the chain (shim/grub payloads on the ESP and the
  NVRAM boot entry) is checked and summarized.
- **Still not implemented**: GRUB module/font installation (a real
  grub-install builds a standalone image from a module directory), ext4
  writes (all repairs remain ESP/NVRAM-only).

## 10. Minimal Shell v1 (built-in)

```
hwdiag      re-run diagnostics          lsdev   device list
lsos        identified systems          lsmnt   mount table
mount / umount / cat / help
bootinfo    per-disk boot details       diskhealth [-scan]
grub-fix    Linux boot repair (diagnose / repair / install)
crypto-selftest  SHA-256/RSA known-answer tests (M8.1)
```

Shell roadmap: built-in minimal -> ash -> bash. (bash: C, POSIX-sh superset, smaller
than fish; job control and readline come late.)

**Implemented (v1, `kernel/src/shell/`)**: serial line editor (echo,
backspace, 128-byte lines; `Serial::read` polls LSR), tokenizer, command
table with per-command handlers — `shell/mod.rs` owns input/dispatch/mount
aliases, `shell/cmds.rs` the commands and their data sources (diag stages,
PCI catalog, probe table, SMART, BootInfo). The loop halts between polls, so
the demo tasks keep running; after 30 s without input it logs
`shell: idle on serial (no input yet)` once. Two safety rules are enforced:
`mount` only creates read-only aliases of the already-mounted FAT32 (any
other request is refused), and every write action (`grub-fix repair`,
`grub-fix install`) requires an explicit `YES` line before it calls
`enable_repair_mode()` — a grep audit confirms the shell contains exactly
one such call. Autorun: an ESP script
(`EFI/fantuan/shell.cmd`, one command per line) is executed at shell start,
which is also how the smoke test drives the commands headlessly.

## 11. Repository Layout (planned)

```
fantuan-kernel/
├── docs/            # design & specs (English)
├── boot/            # EFI app (Rust) + assembly entry stubs + linker scripts
├── kernel/          # Rust core: mm, sched, vfs, syscall, diag; arch/x86_64 (+riscv64)
├── drivers/
│   ├── c/           # C drivers + rust_core.h (AHCI, NVMe, ...)
│   └── rust/        # simple/new drivers
├── userspace/       # future: own libc, shell, tools
└── tools/           # build & QEMU scripts (OVMF + VVFAT one-shot run)
```

## 12. Milestones

- **M0** — DONE: UEFI boot chain (hand-rolled EFI app) + assembly entry + BootInfo
  handshake + serial/GOP console + beeper. Boots in QEMU/OVMF.
- **M1** — interrupts & exceptions: IDT + 256 ISR stubs, exception handlers,
  PIC remap, PIT periodic timer + IRQ0 ticks, TSS/IST double-fault stack,
  TSC-calibrated sleep. Also lands the asm_defs.inc constant pipeline (§13.3).
- **M2** — DONE: higher-half kernel (PHYS_OFFSET, bootloader-built initial
  tables), kernel-owned page tables + CR3 switch, bitmap frame allocator over
  the EFI memory map (505 MiB usable in the QEMU VM), frame self-test, reclaim
  of the bootloader's tables.
- **M3** — DONE: kernel tasks + round-robin scheduler (assembly context
  switch, 100 ms quantum, tick-deadline sleeping) + own syscall ABI v1
  (INT 0x60, versioned dispatch table, SYS_VERSION probe).
- **M4** — DONE: ring-3 user mode (GDT user segments, TSS rsp0, DPL-3
  syscall gate), per-task page tables, static-ELF loader, first userland
  program (fantuan-user) running and exiting cleanly.
- **M4.5** — DONE: C/Rust driver boundary (rust_core.h v1 + driver.h), PCI
  enumeration in the Rust core, first C driver: AHCI read-only (polling, one
  command slot) reading a QEMU test disk. NVMe is the next storage driver.
- **M5** — DONE: diagnostics framework v1 (Check/Severity/stage runner) with
  stage 1 (CPU/GPU/RAM) and stage 2 (storage via AHCI: boot header +
  IDENTIFY); beep codes wired.
- **M5.5** — DONE: SMBIOS parser (`kernel/src/smbios.rs`: config-table entry
  point with F-segment fallback, checksum-validated, types 0/1/4/9/17 bounded
  walk, printable-ASCII string pool) feeding the CPU identity lines and the
  slot-vs-PCI dGPU check; driver ops extension (`blk_open`/`blk_identify`/
  `blk_smart_read_data`/`blk_smart_read_log`, handle-validated in the C
  driver); SMART disk health (`diag/diskhealth.rs`: power-on hours,
  reallocated/pending/uncorrectable, ATA SSD detection via IDENTIFY word 217,
  4 GiB-capped opt-in surface scan kept for the shell); filesystem probing
  (`vfs/probe.rs`: FAT32 mount + ext2/3/4, XFS, Btrfs, NTFS, swap magic,
  probe-only labelling, ESP bootloader enumeration); PCI catalog
  (`pci.rs`: display/storage lists, device struct with BAR0/BAR5). Verified by
  QEMU `-smbios` overrides. (The `--two-fs` fixture's ext4-magic stub has
  since become the real ext4 root of M6.5; surface scan and NVMe SMART landed
  with §10 and M8.)
- **M6** — DONE (read-only core): VFS v1 (mount of the first FAT32 partition),
  GPT + MBR parsing, FAT32 read-only driver (BPB, FAT chain walk, 8.3
  directory entries with LFN skipping, multi-cluster file reads) over the C
  AHCI driver. The test disk is generated by tools/mkdisk.py (hand-built
  GPT + FAT32, zero host deps). Deferred at the time: FAT32 writes (landed in
  M7.5b), ext4/UFS read-only (ext4 landed in M6.5), NTFS (M6.5+).
- **M6.5** — DONE (ext4 read-only): the `vfs/ext4/` driver (split per the
  file-size rule into `mod.rs` types + `sb.rs` superblock/inodes + `dir.rs`
  lookup/read + `extents.rs` block mapping) parses 32/64-byte group
  descriptors, walks extent trees to depth 0/1 (uninitialised extents read as
  zeroes) with the classic ext2/3 map as fallback, and performs path lookup,
  directory listing and file reads for block sizes up to 4 KiB. No journal
  replay, no writes: a dirty filesystem is reported as "needs journal
  recovery" and shown read-only. `vfs::init` mounts the first parseable ext4
  root ro at `/mnt/root0` (magic-only stubs fail parse and stay probe-only);
  the shell's `cat` falls through to ext4 for non-8.3 paths and `lsmnt` lists
  the root; boot repair prefers the real `/etc/fstab` from the ext4 root and
  cross-checks its UUID against grub.cfg's `search.fs_uuid`. The `--two-fs`
  fixture now hand-builds a real ext4 root (1 KiB blocks, extents,
  /etc/fstab) in `tools/mkdisk.py`, with XFS magic on a third partition to
  prove unsupported filesystems stay probe-only.
- **M7.5b** — DONE (repair writes): vfs::write_file behind an explicit
  repair-mode gate (the rescue iron rule), FAT32 write path
  (kernel/src/vfs/fat_write.rs: cluster allocation, both FAT copies,
  data→FAT→dir-entry ordering, overwrite-in-place), bootrepair self-test
  (FIXED.TXT write+readback) and the fallback-loader repair (shim copied into
  a missing EFI/BOOT/BOOTX64.EFI; tools/mkdisk.py --broken builds the
  broken-ESP fixture). Deferred: NVRAM SetVariable repair (M7.6), directory
  create/delete, ext4 repair.
- **M7.5a** — DONE: NVRAM reads via Runtime Services (BootInfo carries the RT
  pointer; kernel/src/runtime.rs; BootCurrent self-test, Secure Boot/SetupMode,
  BootOrder + Boot#### with ESP stale-entry checks, firmware clock).
- **M7** — DONE (diagnosis v1): bootrepair module — ESP scan with bootloader
  identification (fallback loader, Ubuntu shim/GRUB chain, Debian, systemd,
  Windows identified-not-repaired), grub.cfg parsing (search.fs_uuid, root
  device), fstab parsing (UUID=/PARTUUID=), cross-checks (PARTUUID vs GPT
  unique GUIDs, fstab/ vs grub.cfg UUID consistency). Read-only: repairs are
  recommendations until FAT32 writes (M7.5b). Deferred: NVRAM/Secure Boot
  checks (need UEFI Runtime Services via BootInfo), BSD UFS diagnosis,
  actual repair actions.
- **M7.6** — DONE (NVRAM repair via SetVariable): explicit-attrs SetVariable
  (NV|BS|RT for creates, attrs=0 for deletes), Boot#### device-path parsing
  (HD node GPT signatures, FilePath nodes, PCI nodes correlated with the
  driven AHCI controller so whole-disk firmware entries count as covering
  the ESP), and the repair actions behind repair mode — drop stale ESP
  entries from BootOrder and delete them, ensure an explicit partition-level
  entry (HD + FilePath + End device path built from the partition table,
  created only when the ESP fallback exists and verified by re-reading;
  idempotent via GPT-signature matching). Key firmware discovery: the
  variable policy locks the namespace at ReadyToBoot — only boot variables
  are writable, which matches the repair scope exactly. Tested with QEMU
  q35 + SMM OVMF (tools/run.sh --smm; smoke boots the broken-ESP fixture
  three times — delete, create, keep — against one persistent vars store).
  Deferred: Secure Boot key enrollment (M7.7), per-vendor firmware quirks.
- **M7.7** — DONE (Secure Boot key enrollment, Setup-Mode path): Secure
  Boot state is read through the enumeration path (direct-name GetVariable is
  unreliable on some firmware — OVMF reported the variables absent while
  enumeration shows Secure Boot disabled + SetupMode ACTIVE). When the
  firmware is in Setup Mode the rescue system can enroll the platform key
  (and KEK/db) unauthenticated, as the UEFI spec allows: certificates are
  read from the ESP (\EFI\fantuan\PK.cer etc.), written with NV|BS|RT, and
  verified by re-reading; the log warns that Secure Boot becomes enforced on
  the next boot. Authenticated updates once a PK exists need a PKCS#7 /
  SHA-256 stack — deferred to the crypto milestone (M8), together with the
  linuxulator's Secure Boot story.
  **Verification status**: the report/inventory/enrollment-attempt path is
  smoke-tested (SetupMode ACTIVE, PK/KEK/db absent, certificate read from the
  ESP, honest refusal log). The actual key write cannot land in the local test
  environment: the keyless OVMF vars template is a *plain* (non-auth) variable
  store, which rejects Secure Boot variables with EFI_INVALID_PARAMETER even in
  Setup Mode, while the auth-store template ships enrolled keys and would
  refuse our unsigned loader. Full verification needs an OVMF build with
  SECURE_BOOT_ENABLE=TRUE and an empty auth store (or the same on real
  hardware) — the code is written for exactly that case.
- **M7.8 / §10** — DONE (Minimal Shell v1): the built-in shell described in
  §10 — serial line editor, the initial 11 commands (help, hwdiag, lsdev,
  lsos, lsmnt, mount, umount, cat, bootinfo, diskhealth [-scan], grub-fix
  [diagnose|repair|install]; crypto-selftest joined in M8.1), surface scan
  with progress + 'q' cancel and the 4 GiB
  cap, confirmation-gated repair, ESP autorun script. Split across
  shell/mod.rs (input, dispatch, aliases) and shell/cmds.rs (commands) per the
  file-size rule. Deferred: ash/bash, job control, keyboard input (the input
  layer takes the serial reader, so a PS/2 path plugs in).
- **M7.9** — DONE (boot repair v2): the repair write contract of §9.1 plus the
  pieces it needed — `/boot` inventory and `os-release`/`default/grub`
  parsing from the mounted ext4 root (`bootfiles.rs`), systemd-boot/rEFInd/
  EFI-stub (UKI) diagnosis in `esp.rs`, a deterministic `grub.cfg` generator
  (`grubgen.rs`), FAT directory creation (`vfs/fat_dir.rs`), and
  `grub-fix install` (`install.rs`): target selection (ubuntu → debian →
  EFI/fantuan), verified `GRUBCFG.BAK`, publish + byte-compare, fallback
  chain and NVRAM entry. The prelude split `nvram.rs`, `ahci.c`/`nvme.c` and
  `interrupts.S` to the size rule and gave `frame::free` ownership checks.
  Verified by a new smoke phase (backup 108 bytes, generated 336 bytes, both
  re-read), plus `--nvme --grub-regen` and `--smm --grub-regen` (the latter
  creates the explicit partition-level Boot#### through SetVariable).
  Still missing (documented in §9.1): GRUB module/font installation and
  ext4 writes.
- **M8 (storage part)** — DONE: block-device ops registry (`drivers/c/blk.c`)
  + NVMe driver (`drivers/c/nvme.c`) behind the same ops table as AHCI;
  driver-side identity decode (ATA IDENTIFY / NVMe Identify Controller +
  Namespace) surfaced as `blk_identity`; NVMe SMART/Health log decoding in
  `diag/diskhealth.rs` (data units are 1000 x 512 bytes; unimplemented
  fields read as all-ones and are reported as zero); dynamic MMIO mapping for
  64-bit BARs (`mm::paging::map_mmio`). Verified by `tools/run.sh --nvme`:
  the whole stack (FAT32 VFS, boot repair, diagnostics, SMART) runs over NVMe
  with the AHCI path unchanged. Still open for M8: the linuxulator
  compatibility layer and a crypto stack (PKCS#7/SHA-256) for authenticated
  Secure Boot variable updates.
- **Audit rounds (P0-P2)** — DONE (code only; no new subsystem): the M1-M8
  read-only-rule and robustness audits. P0 consent-gated every repair
  (`diagnose()` vs `repair()`, streaming FAT writes, BPB-driven FAT copies);
  P1 fixed the beep classification, the NVMe doorbell stride, task/IRQ
  boundaries and table-overflow paths; P2 hardened the surviving findings:
  FAT chain cycle caps and buffer clamps (a crafted ESP can no longer panic or
  hang the boot), ELF header/segment validation and kernel-half vaddr
  rejection, firmware-returned size clamps, AHCI LBA48 + a real FIS receive
  page, `alloc_cluster` failure propagation, full-namespace BootOrder
  enumeration, contiguous frame allocation for stacks, the TSS I/O-map (ring 3
  has no port access), the `RepairToken` write capability, NVMe MQES/timeouts,
  checked `map_mmio` arithmetic, MBR range guards, correct CPUID 0xB topology,
  and a memory-map grow-and-retry in the bootloader's ExitBootServices loop.
  Verified by the smoke suite plus the `--liar` (oversized FAT entry) and
  `--bigcluster` (4 KiB clusters, short cluster write) fixtures. Deferred to a
  later cleanup: splitting `nvme.c`/`ahci.c`/`interrupts.S` to the §13.4a size
  rule, and ownership validation in `frame::free`.
- **M8.1** — DONE (crypto stack + authenticated variables): the
  self-contained crypto needed to apply operator-provided `.auth` bundles
  when a platform key already exists (the firmware enforces the real chain;
  the kernel only proves internal consistency before writing).
  - `crypto/sha256.rs` (FIPS 180-4), `crypto/bigint.rs` (fixed-limb
    Montgomery arithmetic, modexp), `crypto/rsa.rs` (PKCS#1 v1.5 SHA-256
    verification). Known-answer tests run at boot and via the
    `crypto-selftest` shell command: FIPS hash vectors plus an
    openssl-generated RSA-2048 signature vector embedded in
    `crypto/vectors.rs` (positive + corrupted-signature negative).
  - `crypto/{der,x509,pkcs7}.rs` (minimal DER, SPKI extraction/self-verify,
    SignedData parse) and `bootrepair/secureboot_auth.rs`: scan the ESP for
    `PK/KEK/DB` `.auth` bundles (FAT exposes them as `PK.AUT` etc. — 8.3
    short aliases), parse EFI_VARIABLE_AUTHENTICATION_2 (EFI_TIME +
    WIN_CERTIFICATE_UEFI_GUID), verify the PKCS#7 signature against the
    embedded certificate and the appended payload, then apply with
    TIME_BASED_AUTHENTICATED_WRITE_ACCESS and report the firmware status.
  - Verification levels are explicit: structure and self-consistency are
    checked in the kernel; chain trust is decided by firmware. The smoke
    `--smm` phase asserts descriptor parse, PKCS#7 self-verify, self-signed
    signer certificate, and the honest rejection on a plain variable store.
    `tools/gen_auth_fixture.sh` regenerates the fixture blob (OpenSSL,
    development-time only); the UEFI layout was confirmed against EDK2
    AuthService.c (descriptor followed by the new variable value).
- **M8.3** — DONE (kernel hardening):
  - M8.3a `mm/lock.rs`: every mutating frame-allocator entry point takes an
    IrqLock (spinlock acquired with interrupts disabled), the prerequisite
    for freeing frames from the IRQ0 scheduler path.
  - M8.3b task reaping (`task/reap.rs`): schedule() frees the kernel stack
    of every Exited task except the caller's own slot and tears down its
    user address space via `mm::user::free_user_pml4` (user half only, the
    shared kernel PML4 entry untouched); the slot returns to Unused. Closes
    the M4 "exited tasks leak stacks + page tables" note.
  - M8.3c W^X + SMEP/SMAP: EFER.NXE is enabled before the ELF loader maps
    anything; PT_LOAD protections come from `p_flags` (writable only with
    PF_W, NX unless PF_X; shared pages take the union of rights), the user
    stack is NX, and CR4.SMEP/SMAP are set when CPUID advertises them.
    `sys_write` brackets its user-pointer copy with STAC/CLAC. Tools:
    run.sh passes `-cpu max` so the features are actually exercised; smoke
    asserts "mmu: nx true smep true smap true" plus both reap lines.
- **M8.5** — DONE (input):
  - M8.5a: i8042 PS/2 keyboard (C driver: controller bring-up, IRQ1, ring
    buffer, set-1 translation with shift) feeding a merged serial+keyboard
    input source (`input.rs`); the C layer uses new `k_inb`/`k_outb`
    rust_core exports instead of raw asm. IRQ1 is unmasked after PIC init;
    absent hardware degrades to serial-only.
  - M8.5b: the console moved to a global state and serial output is mirrored
    onto it once the shell starts, so shell interaction (and everything the
    shell prints) is visible on the GOP. The console accesses the
    framebuffer through the PHYS_OFFSET alias — with a user task's CR3 the
    identity-mapped physical address is absent, and a mirrored sys_write
    used to recurse through the page-fault handler; the alias is present in
    every address space.
  - Verified by tools/kbd_test.sh: QEMU monitor `sendkey` injects "zz" +
    Enter after the shell starts (the shell answers the unknown command)
    and two screendumps must differ (mirror output reached the GOP).
    smoke.sh runs it as a phase; all QEMU invocations now get stdin from
    /dev/null (inherited stdin was feeding the guest serial).
- **M8** — linuxulator compatibility layer + Secure Boot story.
- **M9** — RISC-V port behind the arch/ HAL: roadmap in §14 (planned; the
  x86_64 rescue system stays the primary target).

## 13. Governing Principles

1. **Append-only ABIs** — every boundary protocol (BootInfo, syscalls, ops tables)
   only gains fields; deprecate by marking, never by breaking.
2. **Capability negotiation** — features appear/disappear via capability bits, not
   version guessing.
3. **Magic numbers live in exactly one place** — `kernel/src/consts.rs` is the
   single source; `build.rs` generates `asm_defs.inc` from it (assembly
   `#include`s the .inc; raw hex in assembly is forbidden) and also generates
   the `isr_table.rs` extern-stub table. Assembly files compile via the `cc`
   crate. (The historical "magic number hell" — solved structurally.)
4. **Modularity by construction** — cargo features / config for compile-time
   add/remove; the core (mm/sched/vfs/syscall) is always minimal.
4a. **Files stay small (no shit-mountains)** — target ≤ ~300 lines per file;
   when a file grows beyond that, split it and leave a module-map comment at
   the top of the remaining file. Every file opens with a header comment
   stating what it owns; logical blocks are marked with section banners
   (`// --- ... ---` in Rust, `/* --- ... --- */` in C). Splitting early is
   cheap; cleaning up a monolith later is technical debt.
5. **Documentation first** — design changes land here before code.
6. **English-only artifacts** — see header.

## 14. RISC-V Port Roadmap (M9, planned)

Status: planned, not started. The x86_64 rescue system remains the product;
the RISC-V port proves the arch/ split and keeps the core portable. The
reference platform is QEMU `virt` with OpenSBI (`qemu-system-riscv64`,
`-bios default`).

### 14.1 Environment facts (verified 2026-09)

- `riscv64gc-unknown-none-elf` is installed via rustup (the system cargo
  lacks no_std targets, same as for x86).
- `qemu-system-riscv64` 10.2.3 is installed; **x86_64 must stay in
  `QEMU_SOFTMMU_TARGETS`** so the primary platform's smoke suite keeps
  running (a riscv-only rebuild removes `qemu-system-x86_64`).
- OpenSBI `opensbi-riscv64-generic-fw_dynamic.bin` ships with QEMU
  (`-bios default`, OpenSBI v1.7 reported).
- Verified by an out-of-tree spike (2026-09): an ELF linked at
  `0x80200000` (rustc code model medium = LLVM medany) boots with
  `qemu-system-riscv64 -machine virt -bios default -kernel hello.elf
  -nographic`, and the NS16550 MMIO UART at `0x10000000` prints.
- Measured in-tree (M9.1): a0 = hartid = 0, a1 = DTB = 0x87e00000 (DTB near
  the top of the 128 MiB RAM), `/memory` node = 0x80000000..0x88000000;
  OpenSBI reserves its runtime via the DT memreserve block. Sv39's
  canonical high half starts at 0xFFFFFFC0_00000000; the kernel builds
  identity + alias with 2 MiB leaves and enters the high half.

### 14.2 Why this is a port, not a recompile

- The UEFI bootloader (`boot/`) is x86-specific and unused on RISC-V:
  OpenSBI enters the kernel in S-mode with `a0 = hartid`, `a1 = DTB`, so a
  new early entry path is required.
- `PHYS_OFFSET` is an x86 canonical address; Sv39's high half starts at
  `0xFFFFFFC0_00000000`. The ABI constant becomes arch-conditional at
  compile time and the linker script/base move with it. No wire-format
  change: BootInfo stays, but gains an arch/boot-kind discriminator.
- No GOP, no ACPI, no SMBIOS: the console is the NS16550 MMIO UART, memory
  comes from the FDT, and the SMBIOS-based identity/DIMM/slot diagnostics
  degrade to FDT-reported values where they exist at all.
- PCI is ECAM (the Rust config-space reader gains an MMIO path); QEMU virt
  storage is `virtio-blk`, not AHCI/NVMe — a new C driver behind the
  existing `blk_ops` registry, so VFS/diagnostics/boot repair stay
  unchanged (the registry's payoff).
- Interrupts are PLIC + SBI/CLINT timer; context switches save ra/sp/s0-s11;
  syscalls use `ecall`. `kernel/src/consts.rs` (and thus `asm_defs.inc`) is
  x86-flavored and gains arch-selected output.

### 14.3 Phases

1. **M9.0 — arch/ HAL, no behaviour change (IN PROGRESS).** Moved under
   `kernel/src/arch/x86_64/` with crate-root re-exports so no call site
   changed: cpu, exceptions, gdt, idt, interrupts, pic, pit, port, syscall,
   tsc, serial, pci, and the x86 page tables (`paging`/`user`, still
   reachable as `crate::mm::paging`/`crate::mm::user`). Every slice keeps
   the full smoke suite green. Still x86-shaped and to be cfg-split when
   riscv64 lands: task entry/TSS/iretq, the ELF loader's PHYS_OFFSET use,
   the C input driver (i8042, port I/O), runtime/SMBIOS/UEFI boot. The
   explicit trait interfaces are deferred until a second implementation
   forces their shape.
2. **M9.1 — boot.** Kickoff checklist (spike-verified): riscv64 build
   target, link at `0x80200000` with a riscv linker script and `.cargo`
   rustflags, `_start` that first writes the MMIO UART banner (proving the
   handoff), then FDT-lite memory map (memory + reserved-memory only),
   Sv39 early mapping and a riscv BootInfo (arch discriminator, DTB-based
   memmap, no framebuffer/ACPI). The x86 smoke suite must stay green in the
   same tree via `#[cfg(target_arch)]`.
3. **M9.2 — traps and tasks.** SBI timer, PLIC external IRQs, trap frame,
   context switch, `ecall` syscall dispatch, kernel tasks + scheduler.
4. **M9.3 — user mode and diagnostics.** U-mode tasks with per-task page
   tables, FDT-based CPU/RAM diagnostics, PCIe ECAM catalog.
5. **M9.4 — storage.** virtio-blk C driver registering `blk_ops`; then the
   VFS, probe table, SMART and boot repair run unchanged.
6. **M9.5 — verification.** `tools/run.sh --arch riscv64` plus a smoke
   variant on QEMU virt (bounded run, greps for the handshake and VFS).

### 14.4 Risks and mitigations

- **Arch refactor destabilises x86**: cfg-gate only, one subsystem per
  commit, full x86 smoke after each; no functional changes mixed in.
- **ABI drift**: append-only BootInfo field for the arch/boot kind;
  `PHYS_OFFSET` stays a compile-time constant per architecture.
- **FDT scope creep**: no general FDT library — memory/reserved nodes first,
  more only when a consumer exists.
- **virtio-blk complexity** (queue setup, feature negotiation): first cut is
  polling, modern (non-legacy) transport, one queue pair — the same shape
  as the NVMe driver.
- **Unknown hardware behaviour**: everything is spiked on QEMU before it is
  coded; no register offsets from memory (the M8.1 workflow).
