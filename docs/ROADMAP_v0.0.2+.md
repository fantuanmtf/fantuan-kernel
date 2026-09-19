# Roadmap v0.0.2 -> v1.0.0

> Status: living document. This extends DESIGN §12 and the M9 plan
> (`M9_KERNEL_v0.0.1.md`). Versions map to milestones M10–M16. The rule is
> to release a milestone late rather than half-verified; M15 (desktop +
> isolation) targets v0.1.5 with v0.5.0/v1.0.0 as the hard fallback dates.

## 0. Constraints (apply to every milestone)

1. **Licensing is multi-component**: the project's own code is BSD-3-Clause
   and bundled third-party components keep their own licenses, which spreads
   legal risk. The kernel, drivers and base image still prefer
   BSD/MIT/Apache-2.0 code: the network stack is **NetBSD-derived (BSD-2/3)**
   rather than Linux `net/` (GPL); base userland prefers toybox (0BSD) over
   busybox (GPL) and bmake (BSD)/ninja (Apache-2.0) over GNU make. Clang
   (Apache-2.0 with LLVM exception) is preferred over GCC. Copyleft programs
   (XFCE, Qt, tcc as an executable) ship only in the optional developer image
   as separate programs with their sources, never linked into the kernel.
   `THIRD_PARTY.md` is the register of every component, version, license,
   modification and origin; imported sources keep their upstream headers and
   are exempt from the 300-line rule (our wrappers are not).
2. **Engineering rules** (DESIGN §13): English artifacts, docs before code,
   <=300-line source files, zero warnings, append-only ABIs, the read-only
   iron rule, spike-before-code, evidence in every commit.
3. **Images**: a base rescue image below 1 GB (UEFI + BIOS hybrid ISO) and a
   developer image at most 4 GB (ext4 root). Large toolchains and source
   trees (MinGW, desktop sources) are fetched at runtime by scripts and are
   never embedded.
4. **Verification**: QEMU-first per milestone; hardware claims only after a
   device spike. Each milestone adds smoke phases and keeps every earlier
   architecture green (x86_64, riscv64, then i686, arm64).

## 1. Cross-cutting foundations

| id | Foundation | Needed by |
|---|---|---|
| F1 | Pointer-width audit of kernel-core (u64 addresses -> usize-clean, 32-bit `PHYS_OFFSET`) | M10 |
| F2 | SMP: AP bring-up, per-CPU state, IPIs, spinlocks (APIC / GIC) | M11+ |
| F3 | Kernel heap + demand paging + COW | M13/M14 |
| F4 | POSIX syscall surface + musl port + process model | M14 |
| F5 | Network layering: `net_ops` -> ARP/IPv4 -> TCP/UDP -> TLS -> apps | M11 |
| F6 | Graphics: framebuffer blit/damage, input events, KMS-like contract | M13 |
| F7 | Toolchain chain: seed tcc/nasm -> self-host -> C++ seed (clang) | M14 |
| F8 | Virtualization: V1 detect -> V2 minimal hypervisor -> V3 disk-service VM | M12-M15 |
| F9 | Image builder: hybrid ISO, size-budget check, reproducible outputs | M10 |

## 2. M10 — v0.0.2: legacy boot + 32-bit x86

The "old machine" release: machines without UEFI boot the kernel, and 32-bit
CPUs are supported. Design: `M10_BOOT_32BIT.md` (written before code).

- **BIOS boot chain** (self-written, no GRUB): a 512-byte stage1 in the MBR
  loads stage2, which collects the E820 memory map, enables A20, switches
  protected -> long mode and loads the flat kernel. BootInfo gains
  `arch = 3 (BIOS)`; there are no UEFI Runtime Services, so NVRAM repair
  degrades honestly; the RSDP comes from the EBDA/0xF0000 scan; the console
  is VBE framebuffer when available and serial otherwise.
- **i686 port**: 32-bit paging (no PAE in v1), a 2G/2G or 3G/1G split with a
  per-arch `PHYS_OFFSET`, IDT/PIC/PIT, ring 3, `int 0x80` syscalls, an
  ELF32 loader, and the kernel-core pointer audit (F1).
- **Docs**: "Windows boot repair is not supported" (closed boot chain;
  recommend WinPE), the boot matrix (UEFI/BIOS/OpenSBI) and the support
  matrix.
- **Verification**: SeaBIOS + q35 boots to the shell; an i686 smoke subset
  (VFS, userland, repair gate); x86_64 and riscv regressions stay green.

## 3. M11 — v0.0.3: ARM64 + full TCP/HTTPS

- **aarch64 port**: QEMU `virt` (PL011 UART, GICv3, generic timer, PSCI,
  FDT, TTBR0/TTBR1 MMU, EL0 tasks, `svc` syscalls, virtio-mmio). SMP
  groundwork lands here (F2).
- **Network**: a `net_ops` registry mirroring `blk_ops`; drivers for
  virtio-net (MMIO + PCI) and e1000. Stack: **NetBSD-derived** — a
  rump-style port of `sys/net`/`sys/netinet` plus mbuf/pool/callout into the
  C layer (BSD-2/3; this follows DESIGN §2's intended migration source).
  **TLS: mbedTLS (Apache-2.0)** for HTTPS. *No Linux `net/` code (GPLv2 is
  incompatible with this project).* Design: `M11_NET.md`.
- **Tools**: `ping`, `wget` (HTTP/HTTPS), `nslookup`; NIC link diagnostics.
- **Verification**: arm64 smoke; QEMU user-net and loopback; HTTP(S) GET
  against a fixture server; TLS known-answer tests.

## 4. M12 — v0.0.4: disk tooling + hardware

- **Disk imager**: clone a whole disk to another disk or an image file,
  hash-verified, with progress; writes stay behind `RepairToken` + `YES`.
- **NTFS read-only** driver (MFT, attribute lists; no compression/EFS v1 and
  no write by design — manual Windows repair happens in WinPE).
- **AMD GPU probe** (report-only): PCI identity, BAR mapping, PCIe link
  speed/width, ACPI thermal when exposed, and a conservative VRAM
  reachability test. Deeper vendor-table diagnostics require a documentation
  spike first.
- **Virtualization V1** (F8): CPUID hypervisor detection (KVM/Xen/Hyper-V),
  VT-x/SVM, EPT/NPT, IOMMU presence; print the 2010->modern compatibility
  matrix; document the run-as-guest paths.
- **Verification**: image round-trip hash; NTFS fixture listing; GPU report
  in QEMU and on one real GPU; virtualization report on a bare host and
  inside KVM.

## 5. M13 — v0.0.5: graphics + interface freeze

- Framebuffer abstraction (GOP/VBE first, virtio-gpu later), double
  buffering and damage blits; input event API (PS/2, virtio-input; USB HID
  later).
- **KMS-like ioctl contract** (dumb buffers, page flip, connector info)
  designed so an unmodified Xorg modesetting driver can target it in M15.
- Shared memory + mmap groundwork (F3); the **repair-service IPC contract**
  (versioned, socket-like) that Qt frontends will speak.
- **Verification**: a demo framebuffer app and an IPC client; contracts are
  frozen and documented (`GRAPHICS_API.md`, `REPAIR_IPC.md`).

## 6. M14 — v0.1.0: Linux userspace + hypervisor V2

Design: `M14_LINUXUSERS.md` (written before code).

- **POSIX layer (F4)**: fork/execve/wait4, signals, pipes, futex, mmap/brk +
  COW, tmpfs, minimal `/dev` and `/proc`; a musl port; bmake; bash as the
  default shell (GPLv3, separate program with its sources).
- **Full shell and utilities**: the built-in rescue shell (DESIGN §10) stays
  minimal by design; a complete Bash-style command set arrives as a
  *userspace* program here. bash compiled against musl in-system (or
  shipped in the developer image) is the default shell, registered as a
  separate GPLv3 program with its sources; a permissively licensed `dash`
  may be evaluated as a fallback. Operators on the built-in shell keep the
  twelve rescue commands until then. Tracked as M14-8.
- **Bootstrap (F7)**: the image ships a cross-built seed (tcc + nasm,
  musl, toybox, bmake); in-system, tcc rebuilds itself and then builds NASM,
  after which C userland sources are compiled on target. The C++ seed
  (clang) ships in the developer image. `/bootstrap.sh` is reproducible and
  re-runnable. *C++ cannot be bootstrapped from cc+nasm; this is a recorded,
  deliberate exception.* See `M14_LINUXUSERS.md` §5.
- **Hypervisor V2 (F8)**: VMX/SVM root operation, EPT/NPT, exit handling,
  guest start/stop and guest serial; no device passthrough yet.
- **Verification**: bootstrap reproducibility hash; a toybox/musl test
  subset; a minimal Linux guest boots to its initramfs under our
  hypervisor.

## 7. M15 — v0.1.5: desktop, isolation, MinGW

- **XFCE**: Xorg modesetting on the M13 KMS contract, XFCE components
  compiled in-system with the C++ seed; default theming only (lightness over
  beautification).
- **Qt repair frontend** over the M13 IPC contract: disk/health/boot-repair
  views with exactly the shell's privileges, no shortcuts.
- **Hypervisor V3 (F8)**: a virtio disk backend plus a minimal disk-service
  guest for Qubes-like isolated mounting; when no virtualization exists the
  kernel falls back to physical mounting with a prominent warning; a VM
  console window shows guest output.
- **MinGW (external toolkit)**: `tools/mingw_bootstrap.sh` fetches
  mingw-w64/binutils sources, applies patches and builds to `/opt/mingw` on
  the developer image (runtime download, never embedded). The base system
  still ships only cc + NASM.
- **Verification**: XFCE session screenshot; the Qt app drives a real
  repair; the isolation test follows the threat model; a cross-built
  `hello.exe` runs under Windows/Wine.

## 8. M16 — v0.5.0/v1.0.0: finalize

Complete the virtualization compatibility matrix (nested virt, 2010-era
platforms), broaden drivers and filesystems, run the security audit, verify
the 4 GB image budget, and freeze the v1 documentation set.

## 9. Virtualization compatibility matrix

| Era | Technology | Support intent |
|---|---|---|
| 2006-2009 | AMD-V/NPT, Intel VT-x/EPT | detect + own minimal hypervisor |
| 2008-2010 | VT-d / AMD-Vi (IOMMU) | required for device-isolation claims |
| 2010-2020 | EPT/NPT refinements, nested virt, APICv/AVIC | best-effort detect, functional subset |
| now | KVM / Xen / Hyper-V guests | run-as-guest paths; KVM and Xen recognized first |
| pre-2006 | no hardware virtualization | physical-mount fallback with a warning |

KVM cannot be provided bare-metal (it is a Linux kernel module); the kernel
can run *under* KVM. Xen can be a boot path (Xen hypervisor + this kernel as
dom0) and is the optional host-isolation route. The product path is the
self-written hypervisor above.

## 10. Deferred / non-goals

- Windows boot-chain repair (documented: use WinPE; this project diagnoses
  and repairs Linux/BSD boot chains only).
- ARM32 (only the current ARM64 era is targeted), real-board RISC-V beyond
  QEMU, Wi-Fi, Bluetooth, audio, GPU compute, and any GPL-only component.

## 11. Per-milestone documentation deliverables

Every milestone updates: `DESIGN.md` (design of record), the support matrix
(`USAGE.md`), the test matrix (`OPERATIONS.md`), and adds/extends its design
documents before code. The design set is now complete for M10-M14:
`M10_BOOT_32BIT.md`, `M11_NET.md`, `M12_TOOLS_HW.md`, `M13_GRAPHICS.md`
(plus `GRAPHICS_API.md`/`REPAIR_IPC.md` frozen in M13), and
`M14_LINUXUSERS.md`.
