# M10 Design — Legacy BIOS Boot and the 32-bit x86 Port

> Status: design of record for v0.0.2, written before code (DESIGN §13.5).
> Predecessor: `M9_KERNEL_v0.0.1.md`. Roadmap: `ROADMAP_v0.0.2+.md` §2.

## 1. Goal

Boot fantuan-kernel on machines that have **no UEFI firmware**, and support
**32-bit x86 CPUs**, without regressing the existing UEFI x86_64 or riscv64
paths. The BIOS boot chain and the i686 port are one release because the
old-machine audience needs both.

## 2. Non-goals

- Windows boot-chain diagnosis/repair (documented as unsupported; WinPE is
  the recommendation).
- PAE / >4 GiB physical memory on i686 (v1 uses 32-bit physical addresses;
  PAE is a later option if a user needs it).
- BIOS boot for riscv (riscv has no BIOS; OpenSBI remains the path).
- Replacing the UEFI bootloader: UEFI stays the default x86_64 path.

## 3. Boot matrix after M10

| Firmware | Loader | BootInfo arch | Console | NVRAM repair |
|---|---|---|---|---|
| UEFI (x86_64) | `boot/` (self-written) | 1 | GOP + serial | yes (Runtime Services) |
| BIOS (i686/x86_64) | new `boot-bios/` | 3 | VBE + serial | no (degrades) |
| OpenSBI (riscv64) | firmware `fw_dynamic` | 2 | NS16550 MMIO | no (degrades) |

## 4. BIOS boot chain

Self-written, no GRUB/syslinux dependency (project principle: the boot path
is ours). Two stages, both assembly:

1. **stage1 (512 B, MBR)**: relocate itself, load stage2 from a fixed LBA
   range (the image builder records the LBA in the MBR at build time),
   verify a magic/checksum, jump.
2. **stage2 (real mode -> long mode)**:
   - `int 0x15, e820` memory map collection (bounded array, entry count in
     BootInfo);
   - A20 enable, GDT with flat 32/64-bit descriptors;
   - VBE mode set (`int 0x10, 0x4F02`) when available, else serial-only;
   - load the flat kernel image (`objcopy -O binary`) below 4 GiB and enter
     long mode (identity map for the loader, `EFER.LME`, CR0/CR4), then jump
     to the kernel entry with a BIOS-specific BootInfo pointer;
   - no RSDP from firmware: scan `0xE0000..0xFFFFF` for the ACPI signature;
   - no Runtime Services: `runtime_services = 0` (all NVRAM paths already
     degrade honestly on riscv; the same code paths apply).
3. **Image format**: a hybrid ISO (El Torito BIOS boot image + a FAT EFI
   System Partition) so one artifact boots both firmware types. `F9` in the
   roadmap covers the image builder and its size check.

BootInfo changes are append-only: `arch = 3`, plus a `bios_boot_drive`
byte for diagnostics (nothing else needs firmware state).

## 5. i686 port

The kernel is currently 64-bit only: pointer-width assumptions live mostly
in `kernel-core` (frame allocator, paging helpers, task stacks) and in
`abi::PHYS_OFFSET`.

1. **F1 pointer audit (first task)**: make address arithmetic use `usize`
   where a pointer is involved and a wrapper type for physical addresses,
   so a 32-bit build is a compile-time check rather than a rewrite.
2. **Memory model**: 3G/1G split (kernel high half at `0xC000_0000`,
   `PHYS_OFFSET = 0xC000_0000`), 32-bit paging with 4 KiB + 4 MiB pages, no
   PAE; the frame allocator caps at the first 4 GiB of the E820 map and logs
   the truncated remainder.
3. **Core**: GDT/TSS/IDT, PIC/PIT, context switch (32-bit callee-saved set),
   PCID-free TLB handling, PMC-free port I/O (same as x86_64).
4. **User mode**: ring 3 via `iret`, ELF32 loader in `kernel-core` (the ELF
   checks gain `EM_386`), per-task page directories, `int 0x80` syscalls
   (keep the existing ABI numbers; the arch entry differs only in the frame
   layout and the register name mapping).
5. **C driver layer**: compile the same C sources with `-m32`; the AHCI/VBE
   paths are pointer-width clean already (fixed-width types); audit
   `drivers/c` for `uintptr_t` assumptions.

## 6. Work breakdown (suggested commits)

| Step | Deliverable |
|---|---|
| M10-1 | `boot-bios/` stage1+stage2 with serial output; boots a stub that prints and halts in QEMU SeaBIOS |
| M10-2 | BootInfo `arch=3` + E820 -> memmap synthesis; kernel x86_64 boots from BIOS (serial-only console) |
| M10-3 | F1 pointer-width audit in kernel-core, no behavior change on 64-bit (all smokes green) |
| M10-4 | i686 build target, memory model, core bring-up to serial + scheduler |
| M10-5 | i686 user mode (ELF32, `int 0x80`) + VFS from the test disk |
| M10-6 | VBE framebuffer console (optional); hybrid ISO image builder with the 1 GB cap |
| M10-7 | Docs (Windows-unsupported, boot/support matrices), i686 smoke phase, x86_64/riscv regressions |

## 7. Spike checklist (before each step is coded)

- BIOS LBA read via `int 0x13, ah=0x42` on QEMU SeaBIOS; record the exact
  register contract and error handling.
- E820 entry shapes on SeaBIOS: types, gaps above 4 GiB, reserved holes.
- Long-mode entry with an identity map: required CR4 bits (PAE) and the
  `EFER` sequence.
- VBE: whether QEMU's default VGA exposes 1024x768x32 through `4F02`.
- i686 toolchain: `i686-unknown-none` target availability and the C driver
  `-m32` build through the `cc` crate.

### 7.1 Spike results (recorded 2026-09, QEMU SeaBIOS)

- `int 0x13 AH=0x42` with a DAP at `0x0600` loads stage2 from LBA 1 into
  `0x0000:0x8000` on SeaBIOS with a raw IDE image; CF is set on failure and
  the MBR stays exactly 512 bytes. Verified by `tools/smoke-bios.sh`.
- `int 0x15 EAX=0xE820 EDX='SMAP' ECX=24` returns **7 entries** on QEMU/SeaBIOS:
  usable low RAM `0..0x9FC00`, three reserved ranges, usable
  `0x100000..0x8FE0000`, and a **zero-length type-2 sentinel as the last
  entry** — the kernel synthesis must skip size-0 entries.
- Memory layout used by the loader: MBR `0x7C00`, stack top `0x7000`,
  stage2 `0x8000`, E820 buffer `0x9000` (24 B/entry, max 32). COM1 at 0x3F8,
  115200 8N1 initialized from real mode works.
- QEMU invocation for reproduction:
  `qemu-system-x86_64 -machine pc -drive format=raw,file=build/bios.img
  -nographic -no-reboot` (SeaBIOS prints `Booting from Hard Disk..` on the
  serial console).

### 7.2 Long-mode spike results (recorded 2026-09)

- Transition sequence verified under SeaBIOS: fast-A20 (port 0x92), GDT with
  32-bit code/data and a 64-bit code descriptor, `CR0.PE` -> far jump to the
  32-bit segment, page tables at `0x1000/0x2000/0x3000` (PML4 -> PDPT -> PD,
  512 x 2 MiB identity entries, first 1 GiB), `CR4.PAE`, `CR3`, `EFER.LME`,
  `CR0.PG`, far jump to the 64-bit segment, then jump to the payload at
  `0x10000`. The 64-bit stub prints via COM1 and halts.
- The loader image must be padded (now 1 MiB) so sector requests never run
  past the end of the disk — the first attempt failed with `int 0x13` CF set
  because the payload read exceeded the 10-sector image.
- Payload load: 8 sectors (4 KiB) from LBA 9 to `0x1000:0x0000`; the build
  asserts the payload size.

The artifacts are `boot-bios/stage1.asm`, `boot-bios/stage2.asm`,
`boot-bios/stage2_pm.inc`, `boot-bios/longmode.asm` (debug payload),
`tools/build-bios.sh` and `tools/smoke-bios.sh`.

### 7.3 Kernel handoff results (M10-3, recorded 2026-09)

- stage2 now synthesizes the BootInfo in protected mode: E820 -> 40-byte
  `MemoryDescriptor` array (type 7 usable / 0 reserved, zero-length sentinel
  skipped, everything below 1 MiB reported reserved so the kernel never
  reuses the loader), BootInfo at `0x4000` with `arch = 3`, `kernel_base =
  0x1000000`, `stack_top = 0x80000`, `boot_pml4 = 0x20000` and
  `boot_tables_pages = 13`.
- The kernel is loaded with **ATA PIO** (primary master, LBA28, one sector
  per request) to physical 16 MiB, because the 16-bit DAP cannot address
  above ~1 MiB. The build passes the kernel LBA and sector count to NASM;
  the flat kernel is 195,608 bytes (383 sectors) at LBA 33.
- Page tables: identity + `PHYS_OFFSET` alias for 4 GiB, 2 MiB pages,
  shared PD pages (1 PML4 + 8 PDPT + 4 PD = 13 pages).
- Boot log evidence: `handshake ok ... version=2`, `mm: frame allocator
  ready: 510 MiB usable`, `mm: reclaimed 13 bootloader table pages`,
  `userland: hello from tid 4`, `shell: ready`. Framebuffer is absent
  (serial-only console) and SMBIOS/RSDP are 0 in v1; the diagnostics
  degrade accordingly.
- **Old-CPU bug found and fixed**: `sys_write` executed `stac`/`clac`
  unconditionally, which is #UD on CPUs without SMAP (the default QEMU
  `pc` CPU). `cpu::stac/clac` are now gated on the SMAP-active flag set by
  `enable_smap`, so the default-CPU BIOS smoke exercises the no-SMAP path.
- v1 limitations (recorded): BIOS attaches to legacy IDE (PIIX); AHCI/NVMe
  discovery over BIOS is a follow-up. RSDP/SMBIOS scanning on BIOS is not
  implemented yet (fields are 0).

### 7.4 i686 toolchain spike and decision (recorded 2026-09)

Stable rustup has **no `i686-unknown-none`** target. Checked alternatives:

| Option | Result |
|---|---|
| `i686-unknown-uefi` on stable | builds, but artifacts are COFF/PE — unusable for a flat kernel link |
| `i686-unknown-linux-*` | hosted targets (libc/TLS assumptions), wrong for a kernel |
| nightly `-Z build-std=core` + custom target JSON | **works** (spike below) |

Spike evidence: a `no_std` staticlib built with
`cargo +nightly build -Z build-std=core -Z json-target-spec --target
targets/i686-fantuan-none.json` (arch x86, pentium4, SSE/MMX disabled, no
redzone, static relocation, panic=abort), then linked with
`ld -m elf_i386 -T script` at `0x10000`; `objdump` shows clean 32-bit code
(`outb`, `hlt`) with the expected `probe_entry` symbol.

**Decision**: the 32-bit kernel lives in its own crate `kernel-i686/` with a
**pinned nightly toolchain** (a `rust-toolchain.toml` inside that directory)
and the committed `targets/i686-fantuan-none.json`; it is excluded from the
stable workspace build. This is the single nightly exception in the project,
justified by upstream target availability; x86_64, riscv64 and arm64 stay on
stable. The pointer-width audit (M10-4 step 1) still lands first on stable in
`kernel-core` and must keep all existing smokes green.

### 7.5 F1 audit inventory and rules (M10-4a pre-work)

`kernel-core` is width-agnostic except for address handling; the audit makes
"compiles for a 32-bit target" a checkable property without changing
64-bit behavior.

Memory-model decision (i686): **3G/1G split**, `PHYS_OFFSET = 0xC000_0000`,
kernel linked at `0xC010_0000`; the direct map covers physical memory below
1 GiB, so the frame allocator **caps usable RAM at 1 GiB** on i686 and logs
the truncation. BootInfo fields stay u64 (append-only ABI); the arch glue
converts at the boundary.

Rules:

1. Any value used as a pointer/offset into memory must be converted through
   `usize` **after** a range check; an unchecked `as usize` of a u64 field
   is a bug on 32-bit.
2. On-disk/ELF 64-bit fields (file offsets, sizes, LBAs) keep their u64 type;
   only their in-memory *addressing* is range-checked before conversion.
3. `Task.rsp`/`vm_root`/`kernel_stack_top`/`stack_phys` stay u64 (arch glue
   owns their width); `rsp` conversions go through `usize`.
4. `phys_to_virt` is the only place that adds `PHYS_OFFSET`; i686 must not
   alias physical addresses >= 1 GiB.
5. `abi::PHYS_OFFSET` gains an `i686` cfg arm (`0xC000_0000`).

Audit result (M10-4a): after adding the i686 `PHYS_OFFSET` arm and making
the UEFI `RT_BUFFER_TOO_SMALL` constant width-adaptive
(`(1usize << (usize::BITS - 1)) | 5`), `kernel-core` **compiles clean for
the custom 32-bit target** (nightly `-Z build-std=core -Z json-target-spec`)
and the x86_64/riscv64 builds plus the UEFI boot are unchanged.

Inventory (counts at the audit commit): 81 `as usize` sites in 23
`kernel-core` files, concentrated in `vfs/ext4/{dir,sb,extents}.rs`,
`vfs/fat*.rs`, `elf.rs`, `bootrepair/*`, `frame.rs`. The filesystem/ELF
casts are the ones that need explicit bounds checks; bitmap/index casts of
already-`usize` values are fine. `PHYS_OFFSET` is referenced in
`kernel-core` only by `elf.rs` (the user-half check), so the ABI change is
local.

ELF32: the shared `elf.rs` parses ELF64 only. M10-4c adds a sibling
`elf32.rs` in `kernel-core` that consumes the same `Prot`/`UserOps`
abstraction (`e_machine = 3`, ELF32 program headers); `elf.rs` stays the
64-bit path. The per-arch loader selection is compile-time.

## 8. Verification

- **QEMU**: `qemu-system-i386 -machine pc` (SeaBIOS) for i686 and
  `qemu-system-x86_64 -machine pc` for the BIOS x86_64 boot; the existing
  UEFI/riscv runs must stay green.
- **Smoke**: a new `tools/smoke-bios.sh` (or phases inside `smoke.sh`) that
  boots each ISO in BIOS mode, asserts the boot banner, handshake, VFS
  mount, a userland run and the read-only negative greps.
- **Safety**: the repair gate is unchanged; BIOS has no NVRAM, so
  `grub-fix repair` performs FAT repairs only and says so.

## 9. Risks

| Risk | Mitigation |
|---|---|
| Real-mode assembly bugs (the biggest new surface) | stepwise commits, each with a QEMU boot assertion; spike the LBA/E820/VBE facts first |
| Pointer-width regressions in kernel-core | the audit is a behavior-neutral commit with all smokes green before the i686 target exists |
| No Runtime Services breaks repair expectations | docs and shell messages state the degradation; FAT repairs still work |
| Memory fragmentation across three x86-ish paths | BootInfo is the only firmware boundary; the kernel sees one memmap format |
| Scope creep into Windows repair | explicitly out of scope; docs recommend WinPE |
