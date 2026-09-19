# Progress Tracker (v0.0.2 -> v0.1.5)

> Living progress record. Status legend: `[x]` done and verified,
> `[~]` in progress, `[ ]` todo. Every `[x]` names its commit. Design docs
> live in `docs/` and are written before code (DESIGN §13.5). Re-verify
> before claiming a box: run the listed command.

## Overall

| Version | Milestone | Progress | Verified by |
|---|---|---|---|
| v0.0.1 | M0-M9 (x86_64 rescue + RISC-V port) | `[##########] 100%` | tag `v0.0.1` (local) |
| v0.0.2 | M10 legacy BIOS boot + i686 | `[########--] 75%` | `smoke-bios.sh` (2 phases), UEFI/RISC-V smokes |
| v0.0.3 | M11 ARM64 + full TCP/HTTPS | `[#---------] 10%` | design only |
| v0.0.4 | M12 disk tools + NTFS + GPU + virt detect | `[#---------] 10%` | design only |
| v0.0.5 | M13 graphics/input + interface freeze | `[#---------] 10%` | design only |
| v0.1.0 | M14 Linux userspace + bootstrap + hypervisor V2 | `[#---------] 10%` | design only |
| v0.1.5 | M15 desktop + isolation + MinGW | `[----------] 0%` | - |
| v0.5.0/1.0.0 | M16 finalize | `[----------] 0%` | - |

To v0.0.5: roughly **20%** (three of the four milestones have their design
done, M10 is half implemented).

## v0.0.2 - M10 (BIOS boot + i686)

- [x] M10-1 self-written BIOS stage1/stage2 spike (MBR, int 0x13 LBA, E820)
      - commit `c278097`; verify `tools/smoke-bios.sh`
- [x] M10-2 long-mode transition + 64-bit stub (identity tables, EFER)
      - commit `669707f`; verify `tools/smoke-bios.sh`
- [x] M10-3 BIOS handoff to the real x86_64 kernel (BootInfo arch=3,
      ATA PIO kernel load, jump); old-CPU `stac/clac` #UD fix
      - commit `dfaabab`; verify `tools/smoke-bios.sh` (default CPU)
- [x] M10-4 prep i686 toolchain decision + `targets/i686-fantuan-none.json`
      - commit `7fd3836`; verify probe build on nightly
- [x] M10-4a kernel-core pointer-width audit (F1) — the crate now compiles
      clean for `targets/i686-fantuan-none.json` (nightly build-std);
      ABI gained the i686 `PHYS_OFFSET` (0xC000_0000) and the UEFI status
      constant became width-adaptive. Remaining cast hardening for >4 GiB
      on-disk values is tracked as M10-4a2 under M10-4c
- [x] M10-4b1 `kernel-i686` crate + BIOS handoff: protected-mode stub->
      paging (PSE) -> 32-bit BootInfo -> shared frame allocator (510 MiB)
      - verify `tools/smoke-bios.sh` phase 2
- [x] M10-4b2a i686 interrupts: IDT (48 vectors, generated stubs), PIC
      remap, PIT 100 Hz, exception demo with resume; `iretd`/iret-width and
      iret-frame bugs fixed — verify `tools/smoke-bios.sh` phase 2
- [x] M10-4b2b i686 core: scheduler wired and verified (two demo tasks
      rotate, quiet, reap); GDT/TSS landed with M10-4b3a, per-task page
      tables move to M10-4b3b. The 2026-09 blocker was the shared ISR stub,
      not the target JSON - see "Known issues" (now fixed).
- [x] M10-4b3a i686 ring 3: GDT/ring-0-3 segments + TSS.esp0, DPL-3
      `int 0x80` gate, built-in ring-3 stub entered via `iretd`,
      SYS_WRITE/SYS_EXIT through `kernel_core::syscall` - verify
      `tools/smoke-bios.sh` phase 2 (two traps found on the way: the iret
      CS selector needs RPL 3 for the DPL-3 code segment, and SYS_WRITE is
      `(buf, len)` per fantuan-abi, not the Linux fd form)
- [x] M10-4b3b i686 user mode (ELF32): the shared loader is class-aware
      (ELFCLASS32 for EM_386, ELFCLASS64 as before, fields widened to u64
      before the shared checks); per-task page directories for i686 (alias
      PDEs cloned, user half empty, CR3 switched); the shared `user/` crate
      built for the custom target (`tools/build-user-i686.sh`) and embedded;
      user faults (saved CS RPL 3) kill the task - verify BIOS phase 2
      (`userland:` lines, forced `ud2` fault `user fault: tid N killed` and
      both reaps)
- [x] M10-4a2 pointer-width hardening: `mem::to_usize` helper; 14
      on-disk/ELF narrowing sites converted in `elf.rs`, `frame.rs`,
      `ext4/{dir,extents,mod}.rs`, `shell/cat.rs` (checks now run on the
      u64 before narrowing); the remaining 67 are masked/index casts
      classified as safe in the audit - three-target builds zero warnings,
      UEFI boot + BIOS/riscv smokes PASS (13-phase suite at the final
      checkpoint)
- [x] M10-4c i686 VFS on the test disk: PIO ATA primary-channel driver
      (`kernel-i686/src/ata.rs`, LBA28/LBA48 identify, bounded polling),
      the shared `kernel-core::vfs` mounts the same mkdisk fixture (FAT32
      ESP) and the boot prints `fs: part 1`, `vfs: HELLO.TXT`,
      `vfs: INFO.TXT => 1000`, `esp: EFI/BOOT/BOOTX64.EFI` - verify BIOS
      phase 2. Reads only on i686 (write/repair stubs return -1); the smoke
      boots the kernel image as primary slave via
      `build-bios.sh --slave` so the test disk can be primary master,
      while `run-bios.sh --arch i686` stays master (no disk)
- [ ] M10-5 VBE framebuffer console on BIOS (optional; serial is the base)
- [ ] M10-6 hybrid ISO image builder with the 1 GB size check
- [ ] M10-7 docs (Windows-unsupported/PE, boot + support matrices), i686
      smoke phases, x86_64/riscv64 regressions

## v0.0.3 - M11 (ARM64 + network)

Design: `M11_NET.md`.

- [ ] M11-1 shims: mbuf/pool/callout/locks over frames + tick (unit-tested)
- [ ] M11-2 `net_ops` registry + loopback; ping over loopback
- [ ] M11-3 IPv4/ARP/ICMP/UDP on loopback + counters
- [ ] M11-4 TCP + socket layer; loss/throughput tests
- [ ] M11-5 virtio-net (MMIO/PCI) + e1000; DHCP client
- [ ] M11-6 DNS resolver + `ping`/`nslookup`/`wget`
- [ ] M11-7 mbedTLS port + HTTPS + TLS KATs
- [ ] M11-8 aarch64 port bring-up; smoke phases; THIRD_PARTY entry

## v0.0.4 - M12 (tools + hardware)

Design: `M12_TOOLS_HW.md`.

- [x] M12-1 ACPI table walker: RSDP/RSDT/XSDT validation, FADT/MADT
      (enabled CPUs)/DMAR/IVRS presence — verify the x86 boot log lines
      (UEFI only; the BIOS path has no RSDP yet)
- [ ] M12-2 disk imager + `clone` command + hash verification
- [ ] M12-3 bad-sector policy (`--continue`) + report file
- [ ] M12-4 NTFS boot/MFT/attribute/runlist read path (fixture)
- [ ] M12-5 NTFS listing/read + `/mnt/win0`; probe graduation
- [ ] M12-6 AMD GPU report (identity/BAR/PCIe link/thermal)
- [x] M12-7 virtualization detection: hypervisor vendor (CPUID.40000000h),
      VMX (+ IA32_FEATURE_CONTROL lock/enable, EPT/VPID caps), SVM + NPT,
      VT-d/AMD-Vi via DMAR/IVRS, ROADMAP sec. 9 matrix row, TCG caveat and
      the physical-mount fallback verdict — verify the x86 boot log lines
      (KVM/Xen/bare-metal wording still needs a real bare-metal run)

## v0.0.5 - M13 (graphics + interface freeze)

Design: `M13_GRAPHICS.md`.

- [ ] M13-1 `fb_info` + blit/fill/damage; GOP console on top
- [ ] M13-2 double buffer + present + kernel demo app
- [ ] M13-3 input event ring + PS/2 mouse
- [ ] M13-4 dumb buffers + ADDFB/SETCRTC/PAGE_FLIP + events
- [ ] M13-5 EDID sourcing + synthetic fallback
- [ ] M13-6 repair broker + IPC protocol + client demo
- [ ] M13-7 freeze `GRAPHICS_API.md` / `REPAIR_IPC.md` + smoke phases

## v0.1.0 - M14 (Linux userspace)

Design: `M14_LINUXUSERS.md`.

- [ ] M14-1 kernel heap + VMA + demand paging + COW
- [ ] M14-2 POSIX round 1 (mmap/brk/open/read/write/stat/getdents, tmpfs)
- [ ] M14-3 POSIX round 2 (fork/execve/wait4, signals, futex, pipes)
- [ ] M14-4 musl port + toybox + bmake
- [ ] M14-5 seed/self-host chain + `/bootstrap.sh` + reproducibility hash
- [ ] M14-6 C++ seed (clang) in the developer image
- [ ] M14-7 hypervisor V2 (VMX first, then SVM) + guest serial
- [ ] M14-8 full POSIX shell: toybox sh / dash / bash over the native ABI
      (the built-in shell keeps the 12 rescue commands)

## v0.1.5 - M15 (desktop + isolation + MinGW)

- [ ] M15-1 Xorg modesetting on the M13 KMS contract
- [ ] M15-2 XFCE compiled in-system (C++ seed)
- [ ] M15-3 Qt repair frontend over the IPC contract
- [ ] M15-4 hypervisor V3: virtio disk backend + disk-service guest
- [ ] M15-5 physical-mount fallback with the prominent warning
- [ ] M15-6 `tools/mingw_bootstrap.sh` (runtime download, `/opt/mingw`)

## Last verified snapshots

| Date | Check | Result |
|---|---|---|
| 2026-09 | virt detection on UEFI: vendor/VMX/SVM/EPT/NPT/IOMMU + matrix row + fallback verdict; MSR reads feature-gated; Intel-only MSR (microcode) vendor-gated | PASS |
| 2026-09 | ACPI walker on UEFI: 5 tables, fadt/madt, cpus=1 | PASS |
| 2026-09 | `tools/smoke-bios.sh` phase 2 (i686 interrupts: IDT/PIC/PIT) | PASS |
| 2026-09 | serial heartbeats stop after 30 s; interactive shell clean | PASS |
| 2026-09 | `tools/smoke-bios.sh` phase 2 (i686 handoff + frame allocator) | PASS |
| 2026-09 | `tools/smoke-riscv.sh` 3 phases (after the 32-bit ABI split) | PASS |
| 2026-09 | `kernel-core` 32-bit target build (nightly build-std) | PASS |
| 2026-09 | `tools/smoke-bios.sh` (M10-3, default no-SMAP CPU) | PASS |
| 2026-09 | `tools/run.sh` x86_64 UEFI main phase (SMAP active) | PASS |
| 2026-09 | riscv smoke (3 phases incl. repair YES/NO) | PASS |
| 2026-09 | i686 scheduler after the ISR `popad` fix (2 tasks, 500 ticks, quiet) | PASS |
| 2026-09 | i686 ring 3 via `iretd` + `int 0x80` (built-in stub writes and exits) | PASS |
| 2026-09 | W1/M10-4a2 hardening: three-target builds + BIOS/riscv smokes after the `to_usize` pass | PASS |
| 2026-09 | i686 ELF32 userland (shared `user/` crate) + per-task PD + user-fault kill | PASS |
| 2026-09 | i686 PIO ATA + shared VFS on the mkdisk fixture (phase 2 asserts VFS/ESP lines) | PASS |
| 2026-09 | x86 full suite `tools/smoke.sh` 13/13 | PASS (at v0.0.1) |

## Known issues

- **Fixed (2026-09): i686 scheduler "24 bytes per iteration" stack leak**.
  Root cause was the shared ISR stub (`kernel-i686/src/isr_stubs.S`):
  `add esp, 8` ran *before* `popad`, so every exception/IRQ return popped
  the vector/error words into EDI/ESI/EBP/EBX/EDX/ECX/EAX. The exception
  demo still returned to the right address (its `iretd` frame stayed
  aligned), which hid the corruption until kmain used a register again:
  after `int3`, ESI held the saved original ESP, so `init_arch` built the
  `TaskOps` temporary below the live stack pointer, `push`/`call` overwrote
  it, `set_ops` copied the garbage, and `init`'s `call [OPS+0xc]` jumped to
  the 0xc10003b4 return address, re-running the setup in a loop that leaked
  24 bytes per round (8 arg words + 4 call RA + 12 saved regs) until ESP
  hit 0x20000. The earlier ABI/codegen theory was a red herring: i386
  argument lowering is consistent between crates and the JSON data layout
  matches `i686-unknown-linux-gnu`'s. The DF=1 `cld` fix from the same
  investigation is still required. Fix: drop vector+error *after* `popad`;
  `tools/smoke-bios.sh` phase 2 now asserts the demo tasks and their quiet
  lines (PASS).
- **riscv `uart::log_bytes` fault (one-off)**: one repair run (of three)
  crashed with `scause=0xd stval=0x766`; reproduced once more at the W2
  checkpoint (`scause=0xd stval=0x7f8 sepc=0x80200c4e [kernel]`, right
  after the NVRAM BootOrder line during repair) and the immediate rerun
  passed all three phases. Watch item; hunt in M10/M11 follow-ups.

## M10 completion plan

The remaining M10 work (4a2 hardening, 4b3b ELF32, 4c VFS, 6 ISO,
optional 5 VBE, 7 docs/release) is sequenced in `M10_PLAN.md` with the
verification and commit checkpoints for each workstream.

## Next action

**W4 / M10-6**: self-written ISO9660 + El Torito hybrid image builder
(`tools/iso/` + `tools/build-iso.sh`, <=1 GB check) booting the BIOS image
from CD and exposing the ESP for UEFI. W5 (VBE, in scope) and W6 (docs +
0.0.2) follow; the owner pushes and tags after the set is green.
