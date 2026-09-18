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
      - commit `e146a27`; verify `tools/smoke-bios.sh`
- [x] M10-2 long-mode transition + 64-bit stub (identity tables, EFER)
      - commit `8b6a3af`; verify `tools/smoke-bios.sh`
- [x] M10-3 BIOS handoff to the real x86_64 kernel (BootInfo arch=3,
      ATA PIO kernel load, jump); old-CPU `stac/clac` #UD fix
      - commit `83db01a`; verify `tools/smoke-bios.sh` (default CPU)
- [x] M10-4 prep i686 toolchain decision + `targets/i686-fantuan-none.json`
      - commit `32da9a3`; verify probe build on nightly
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
- [ ] M10-4b2b i686 core: GDT/TSS, kernel-owned page tables, scheduler
      (32-bit context switch) — **BLOCKED on a custom-target codegen/stack
      issue**, see "Known issues" below; the glue is written
      (`kernel-i686/src/{task,demo}.rs`, `context.S`) but not wired.
- [ ] M10-4b3 i686 user mode (ELF32, `int 0x80`)
- [ ] M10-4a2 harden the 81 `as usize` sites for >4 GiB on-disk values
      (filesystem/ELF bounds checks; see `M10_BOOT_32BIT.md` 7.5)
- [ ] M10-4c i686 VFS on the test disk (after b3)
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

- [ ] M12-1 ACPI table walker (RSDT/XSDT)
- [ ] M12-2 disk imager + `clone` command + hash verification
- [ ] M12-3 bad-sector policy (`--continue`) + report file
- [ ] M12-4 NTFS boot/MFT/attribute/runlist read path (fixture)
- [ ] M12-5 NTFS listing/read + `/mnt/win0`; probe graduation
- [ ] M12-6 AMD GPU report (identity/BAR/PCIe link/thermal)
- [ ] M12-7 virtualization detection + matrix + smoke phases

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
| 2026-09 | `tools/smoke-bios.sh` phase 2 (i686 interrupts: IDT/PIC/PIT) | PASS |
| 2026-09 | serial heartbeats stop after 30 s; interactive shell clean | PASS |
| 2026-09 | `tools/smoke-bios.sh` phase 2 (i686 handoff + frame allocator) | PASS |
| 2026-09 | `tools/smoke-riscv.sh` 3 phases (after the 32-bit ABI split) | PASS |
| 2026-09 | `kernel-core` 32-bit target build (nightly build-std) | PASS |
| 2026-09 | `tools/smoke-bios.sh` (M10-3, default no-SMAP CPU) | PASS |
| 2026-09 | `tools/run.sh` x86_64 UEFI main phase (SMAP active) | PASS |
| 2026-09 | riscv smoke (3 phases incl. repair YES/NO) | PASS |
| 2026-09 | x86 full suite `tools/smoke.sh` 13/13 | PASS (at v0.0.1) |

## Known issues

- **i686 scheduler (M10-4b2b) blocked**: with the target's default
  `stack-probes: inline`, the kernel dumps kernel-image bytes to the serial
  and `kernel_core::task::init` runs with ESP=0x20000 (the page-directory
  page), faulting on push. Setting `stack-probes: none` removes the wild ESP
  and the scheduler then runs to completion (tasks interleave, 500 ticks),
  but residual binary noise still appears early in the log. Leading suspect:
  the hand-written `i686-fantuan-none.json` target spec (ABI/codegen
  fields); next steps: compare against a known-good bare-metal i686 target
  JSON (e.g. a build-std `x86_64` variant downcast, or add
  `rustc-abi: softfloat`, `main-needs-argc-argv: false`), and bisect the
  spec fields with a minimal binary. M10-4b2a (IDT/PIC/PIT) is unaffected
  and verified.
- **riscv `uart::log_bytes` fault (one-off)**: one repair run (of three)
  crashed with `scause=0xd stval=0x766`; not reproduced since. Watch item.

- riscv repair phase B crashed once (of three runs) with a load fault in
  `kernel_riscv::uart::log_bytes` (`scause=0xd stval=0x766 sepc=0x80200c00`)
  right after the fallback-copy message started printing. Two later runs
  passed. Signature recorded for investigation (stack/static corruption
  candidate; not reproduced yet) — treat as a flake to hunt in M10/M9.5
  follow-ups.

## Next action

**M10-4b2b**: i686 core — kernel-owned GDT/TSS and page tables plus the
32-bit context switch, wiring `kernel_core::task` (spawn/sleep/reap) so the
shared scheduler runs on 32-bit.
