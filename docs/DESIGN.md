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
  u64::MAX-1 = EINVAL. Versioned dispatch table — every call carries its own
  version so the ABI evolves per-call (capability negotiation);
  `SYS_VERSION` (0) probes the ABI version. Calls: exit(1), sleep_ms(2),
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
  SATA-specific loops. M4.5 implements AHCI read-only first (polling, one
  command slot); NVMe second (modern laptops are NVMe-only; a rescue system
  that misses them misses half the field).
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
- **v1 implemented (M5, `kernel/src/diag/`)**: the framework (Check + Severity +
  per-stage runner) and four checks — cpu (CPUID brand/topology/features via
  inline assembly + RDMSR microcode), gpu (PCI presence + §6.2 beep codes),
  ram (pattern test over allocator-borrowed frames), storage (MBR/GPT boot
  header + IDENTIFY model/serial/capacity through the C AHCI driver).
  Deferred to M5.5: SMBIOS parsing (slot-based "2 short" dGPU detection,
  RAM DIMM info), SMART attributes (power-on hours, reallocated/pending,
  TBW), surface scan, OS identification and auto ro-mount (needs filesystem
  probing, M6).

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
   `"NTFS    "`, UFS, swap) — no full filesystem driver needed.
3. Read `\EFI\` of the ESP to list bootloaders; identify Linux by
   `/boot/grub/grub.cfg`, `vmlinuz`, `initramfs`; identify Windows partitions
   by GPT partition-type GUIDs (works with **zero** NTFS driver).
4. Auto-mount, **read-only**: ESP (FAT32) + identified roots at `/mnt/esp0`,
   `/mnt/root0`. Mount failure degrades to "not mounted + reason", never writes.

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
  parsing with UUID cross-checks (M7), NVRAM diagnosis via Runtime Services —
  BootCurrent self-test, Secure Boot + SetupMode, BootOrder + Boot#### list
  with stale-entry checks including the firmware-default fallback path, and the
  firmware clock (M7.5a). Known quirk: OVMF non-SMM rejects runtime NV creates
  through SetVariable (edk2 requires NV|RT at runtime) — SetVariable-based
  repair is deferred to M7.6.
- **Architecture note**: classic boot repair uses `chroot` into the target system —
  impossible without a Linux ABI. v1 covers ~80% with native repair; a long-term
  **linuxulator compatibility layer** (FreeBSD-style) is kept as an open option to
  reach chroot/dracut-class repairs. The ABI design must not block this.

## 10. Minimal Shell v1 (built-in)

```
hwdiag      re-run diagnostics          lsdev   device list
lsos        identified systems          lsmnt   mount table
mount / umount / cat / help
bootinfo    per-disk boot details       diskhealth [-scan]
grub-fix    Linux boot repair (diagnose / repair modes)
```

Shell roadmap: built-in minimal -> ash -> bash. (bash: C, POSIX-sh superset, smaller
than fish; job control and readline come late.)

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
  IDENTIFY); beep codes wired. M5.5 adds SMBIOS, SMART attributes, surface
  scan, OS identification.
- **M6** — DONE (read-only core): VFS v1 (mount of the first FAT32 partition),
  GPT + MBR parsing, FAT32 read-only driver (BPB, FAT chain walk, 8.3
  directory entries with LFN skipping, multi-cluster file reads) over the C
  AHCI driver. The test disk is generated by tools/mkdisk.py (hand-built
  GPT + FAT32, zero host deps). Deferred: FAT32 writes, ext4/UFS read-only,
  NTFS (M6.5+).
- **M7.5a** — DONE: NVRAM reads via Runtime Services (BootInfo carries the RT
  pointer; kernel/src/runtime.rs; BootCurrent self-test, Secure Boot/SetupMode,
  BootOrder + Boot#### with ESP stale-entry checks, firmware clock).
- **M7** — DONE (diagnosis v1): bootrepair module — ESP scan with bootloader
  identification (fallback loader, Ubuntu shim/GRUB chain, Debian, systemd,
  Windows identified-not-repaired), grub.cfg parsing (search.fs_uuid, root
  device), fstab parsing (UUID=/PARTUUID=), cross-checks (PARTUUID vs GPT
  unique GUIDs, fstab/ vs grub.cfg UUID consistency). Read-only: repairs are
  recommendations until FAT32 writes (M6.5). Deferred: NVRAM/Secure Boot
  checks (need UEFI Runtime Services via BootInfo), BSD UFS diagnosis,
  actual repair actions.
- **M8** — linuxulator compatibility layer + Secure Boot story.
- **M9** — RISC-V port behind the arch/ HAL.

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
