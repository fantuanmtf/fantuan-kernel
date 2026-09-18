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
