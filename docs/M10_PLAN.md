# M10 completion plan (v0.0.2)

Status: approved working plan, 2026-09. Tracks the remaining M10 work from
`PROGRESS.md` in dependency order. Each workstream is one commit with a
"Verified:" paragraph; the repo owner pushes after the full set is green.

## Definition of done (v0.0.2)

- Every M10 checkbox in `PROGRESS.md` is closed (or explicitly deferred
  with a tracked reason).
- BIOS phase 1 (x86_64) and phase 2 (i686: paging, allocator, IDT/PIC/PIT,
  scheduler, ring 3, VFS) pass in `tools/smoke-bios.sh`.
- x86_64 UEFI and riscv64 smokes stay green (`tools/smoke.sh`,
  `tools/smoke-riscv.sh`) because shared code changes here.
- Version strings and docs read 0.0.2; boot + support matrices updated.
- Local tag `v0.0.2` prepared after the owner's review (tag only on request).

## W1 - M10-4a2 pointer-width hardening (shared)

Scope: the 81 `as usize` / narrowing sites listed in `M10_BOOT_32BIT.md`
7.5 (ELF and filesystem offsets, sizes, counts). Add checked conversion
helpers in `kernel-core` (e.g. `mem::to_usize(u64) -> Option<usize>`),
replace the sites, and reject >4 GiB values with the existing
`RT_BUFFER_TOO_SMALL`/`SYS_ERR_INVAL` style errors.

- Files: `kernel-core/src/{elf.rs, vfs/*, mem.rs}`, call sites in
  `kernel/src`, `kernel-riscv/src`, `kernel-i686/src`.
- Verify: three kernel builds warning-free; `tools/smoke.sh` (13 phases),
  `tools/smoke-riscv.sh` (3), `tools/smoke-bios.sh` (2).
- Risk: shared-code churn - keep changes mechanical and rerun all smokes.

## W2 - M10-4b3b i686 ELF32 user mode

Scope: teach the shared loader ELFCLASS32 (`EM_386`) keyed off
`UserOps.machine`; give i686 user tasks their own page directory (clone
alias PDEs 768..1024, user half empty) and install it through the existing
`context_switch(cr3)` path; build the shared `user/` crate for the i686
target; turn user faults into task kills (check the saved CS RPL in the
exception path instead of halting).

- Files: `kernel-core/src/elf.rs`, `kernel-i686/src/{user.rs, user_entry.S,
  isr_stubs.S}`, `user/` build script (`tools/build-user-i686.sh` or an
  extension of `tools/build-i686.sh`), `kernel-i686/build.rs` embedding.
- Verify: BIOS phase 2 asserts the real userland lines (write + exit +
  reap); a deliberate user fault kills only that task; UEFI/riscv smokes
  unchanged (loader stays 64-bit for them).
- Risk: ELF32/ELF64 parser duality - keep the `machine` switch explicit.

## W3 - M10-4c i686 VFS on the test disk

Scope: a PIO ATA block driver for the i686 kernel (stage2 already proves
the PIIX path), partition scan and mount of `build/test.img` through the
shared `kernel-core::vfs` (FAT/ESP+ext fixtures), plus the read commands
through the shared shell so the i686 boot matches the x86_64 VFS lines.

- Files: new `kernel-i686/src/ata.rs` (+ block trait glue), `main.rs`
  wiring, `tools/smoke-bios.sh` phase 2 (attach the disk and assert
  `vfs: HELLO.TXT`, `vfs: INFO.TXT => 1000`, `fs: part 1 ...`).
- Verify: phase 2 assertions above; negative read-only checks stay.
- Risk: shared VFS assumes a block interface shaped for AHCI - adapt on
  the i686 side only.

## W4 - M10-6 hybrid ISO image builder (no GPL tools)

Host has no xorriso/mkisofs/genisoimage, and the project avoids GPL build
tools, so write a minimal ISO9660 + El Torito builder:

- Files: `tools/iso/` (small C program built by `cc`, or a documented
  Python script), `tools/build-iso.sh` producing `build/fantuan.iso` that
  boots the existing BIOS image via El Torito and exposes the ESP image
  for UEFI, with the 1 GB size check (fail loudly above the budget).
- Verify: QEMU BIOS boot from the ISO reaches the stage2 banner; QEMU
  UEFI boot from the same ISO reaches the shell; size assertion in the
  script; `tools/smoke-iso.sh` optional.
- Risk: ISO9660 correctness - keep the image minimal and validate with
  QEMU's CD-ROM path (`-cdrom`).

## W5 - M10-5 VBE framebuffer console (stretch, optional)

Scope: VBE mode information via stage2 (VBE 2.0 info block), set one
linear framebuffer mode (e.g. 1024x768x32) and bring up a text console on
it in `kernel-i686`; serial stays the base and the fallback.

- Verify: QEMU `-vga std` boot shows the console; without VBE the kernel
  falls back to serial and all smokes stay green.
- Decision: if the VBE spike is unstable, defer to M12 with a documented
  reason; this item is marked optional in the tracker.

## W6 - M10-7 docs + v0.0.2 release prep

Scope: Windows/PE non-support page (WinPE guidance), boot matrix
(BIOS/UEFI/ISO, i686 limitations), support matrix entries in `USAGE.md`,
test matrix in `OPERATIONS.md`, `DESIGN.md` updates, version strings to
0.0.2, README index, `PROGRESS.md` final ticks and snapshot rows.

- Verify: doc hash references valid (`git log` scan), three smokes green,
  `--help` texts consistent; tag prepared but created only on request.

## Order and checkpoints

1. W1 (hardening) - independent, unblocks nothing but closes the audit.
2. W2 (ELF32) - the last big i686 kernel feature.
3. W3 (VFS) - depends on W2 only for the smoke harness.
4. W4 (ISO) - independent; can run in parallel after W1.
5. W5 (VBE) - stretch after W3.
6. W6 (docs/release) - after all builds are green.

Checkpoint after each workstream: build all three kernels, run the
affected smoke(s), commit with evidence, update `PROGRESS.md`.
