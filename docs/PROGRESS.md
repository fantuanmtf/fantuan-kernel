# Progress Tracker (v0.0.2 -> v0.1.5)

> Living progress record. Status legend: `[x]` done and verified,
> `[~]` in progress, `[ ]` todo. Every `[x]` names its commit. Design docs
> live in `docs/` and are written before code (DESIGN §13.5). Re-verify
> before claiming a box: run the listed command.

## Overall

| Version | Milestone | Progress | Verified by |
|---|---|---|---|
| v0.0.1 | M0-M9 (x86_64 rescue + RISC-V port) | `[##########] 100%` | tag `v0.0.1` (local) |
| v0.0.2 | M10 legacy BIOS boot + i686 | `[##########] 100%` | released as 0.0.2: `smoke.sh` 13/13, `smoke-bios.sh` 2/2, `smoke-riscv.sh` 3/3, `smoke-iso.sh` 2/2 |
| v0.0.3 | M11 ARM64 + full TCP/HTTPS | `[#---------] 10%` | design only |
| v0.0.4 | M12 disk tools + NTFS + GPU + virt detect | `[#---------] 10%` | design only |
| v0.0.5 | M13 graphics/input + interface freeze | `[#---------] 10%` | design only |
| v0.1.0 | M14 Linux userspace + bootstrap + hypervisor V2 | `[#---------] 10%` | design only |
| v0.1.5 | M15 desktop + isolation + MinGW | `[----------] 0%` | - |
| v0.5.0/1.0.0 | M16 finalize | `[----------] 0%` | - |

To v0.0.5: roughly **25%** (M10 is released; M11-M13 have their designs
done).

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
- [x] M10-5 VBE framebuffer console (owner made it in-scope): stage2
      queries VBE (4F00/4F01, controller info prefilled "VBE2"; SeaBIOS
      answers "VESA" so both signatures accepted) and sets
      1024x768x32 with the LFB bit; the 32-bit BootInfo gains append-only
      fb fields (136 -> 160 B, 64-bit layout untouched); the LFB maps at
      the dedicated PDE 767 (VA 0xBFC00000) in the stage2 PD and every
      per-task PD clones 767..1024; the 8x16 ROM font arrives via INT 10h
      1130; `kernel-i686/src/fb.rs` is the 32/24/16-bpp text console with
      serial mirroring and an `fb: unavailable (serial console)` fallback
      - verify BIOS phase 2 plus the headless screendump check
- [x] M10-6 self-written hybrid ISO: `tools/mkiso.py` builds a level-1
      ISO9660 image (PVD, L/M path tables) with El Torito BIOS
      no-emulation (full preload to 0x7C00/0x8000; SeaBIOS rejects AH=42h
      on emulated drives, so HDD emulation cannot serve stage1) and a UEFI
      0xEF entry over a hand-built FAT16 ESP (`tools/mkesp.py`); the CD
      chain copies the kernel from the preload instead of ATA. 1 GiB budget
      enforced in the writer and re-checked on the file - verify
      `tools/smoke-iso.sh` (BIOS + OVMF)
- [x] M10-7 docs (Windows-unsupported/PE page, boot + support matrices),
      i686 smoke phases, x86_64/riscv64 regressions: `docs/WINDOWS.md`
      added; the boot matrix lives in `OPERATIONS.md` §3 and the
      user-facing support matrix in `USAGE.md` §9; `README.md`,
      `BUILD.md`, `HANDOVER.md`, `DEVELOPMENT.md`, `DESIGN.md` (#4/#12/
      #15) and this tracker updated; workspace + banners bumped to
      `0.0.2` (syscall `ABI_VERSION` stays 1); final verification at the
      v0.0.2 checkpoint: three release builds zero warnings,
      `smoke.sh` 13/13, `smoke-bios.sh` 2/2, `smoke-riscv.sh` 3/3,
      `smoke-iso.sh` 2/2 (see snapshots). No tag or push.

## v0.0.3 - M11 (ARM64 + network)

Design: `M11_NET.md`. Batch plan: `M11_PLAN.md` (rump full-subset import,
aarch64 direct + UEFI, net-smoke offline gate with optional Tor 9050
external phase; **owner pushes between R batches**).

- [x] M11-1 rump full-subset vendor + adaptation layer: R1 imported the
      pinned slice (139 files, 9/9 compile) and R2 built `kernel-net`
      (adapter: libkern/atomics, bump+vmem memory over frames, locks/
      sleepq stubs, curlwp/percpu/xcall, PIT callouts, printf/sysctl,
      net glue) linking with 0 unresolved symbols; the in-kernel self-test
      passes (mbuf 12/12, pool 8/8, callout 20) - verify the UEFI `rump:`
      lines plus smoke-bios/riscv
- [x] M11-2 `net_ops` registry + loopback; ping over loopback: R3 extended
      the import with the real ifnet/route slice (`if.c`, `if_loop.c`,
      `route.c`, `radix.c`, `rtbl.c`, `if_stats.c`, `bpf_stub.c`,
      `subr_pserialize.c`; 197 files), attached lo0 with 127.0.0.1/8 and
      pinged it through an mbuf output->pktqueue->ICMP-echo-reply input
      path in `kernel-net`; `tools/smoke-net.sh` asserts the loopback
      markers (`net: lo0 up`, `net: ping ... ok`, `net: icmp echo reply
      ok`, `net: in/out counters`) - R4 replaces the adapter ICMP with
      ip_input.c/ip_icmp.c/in.c
- [x] M11-3 IPv4/ARP/ICMP/UDP on loopback + counters: R4 imported the real
      IPv4 slice (`ip_input.c`, `ip_output.c`, `ip_icmp.c`, `ip_reass.c`,
      `in.c`, `in_pcb.c`, `in_proto.c`, `udp_usrreq.c`, `if_arp.c`,
      checksum/offload files, `if_llatbl.c`, `nd.c`, `subr_hash.c`,
      `subr_once.c`; 246 files) and replaced the R3 stand-ins: lo0
      `if_output` -> pktqueue -> real `ip_input` -> real `ip_output` ping,
      a UDP exchange over real PCBs, and an ARP request/reply self-test on
      a shim ethernet interface; `tools/smoke-net.sh` asserts the R4
      markers - R5 imports TCP + the socket layer
- [x] M11-4 TCP + socket layer; loss/throughput tests: R5 imported the real
      socket/TCP slice (`uipc_socket.c`, `uipc_socket2.c`,
      `tcp_input/output/subr/timer/usrreq/congctl/sack/syncache.c`; 259
      files), replaced the R4 `rump_sock2.c`/`tcp_*` stubs and ran a real
      socket client over loopback: 3-way handshake, a 64 KiB blob with hash
      equality, graceful close, and a second connection with deterministic
      drops exercising retransmission; `tools/smoke-net.sh` asserts the R5
      markers - R6 brings up virtio-net/e1000 + DHCP
- [x] M11-5 virtio-net (MMIO/PCI) + e1000; DHCP client: R6 added the
      interrupt-free QEMU e1000 adapter behind `net_ops` (PCI/BAR0 through
      new `Env` hooks, RX/TX descriptor rings in frame pages, software
      checksums, ethertype demux into `ip_pktq`/`arp_pktq`), a bounded DHCP
      client over the real UDP socket layer with a provisional link-local
      address and real `in_control`/`rtrequest1` lease application, and the
      SLIRP offline phase in `tools/smoke-net.sh` (python HTTP fixture at
      `10.0.2.2:18080`, exact byte/hash assertion); the `workqueue(9)` shim
      became genuinely deferred. virtio-net is deferred to R9 with the MMIO
      transport (documented) - R7 adds the DNS resolver + tools
- [x] M11-6 DNS resolver + `ping`/`nslookup`/`wget`: R7 added a bounded
      DNS A-query client over the real UDP socket layer (id + question-echo
      verification, three 1 s retries, DHCP resolver or explicit override;
      `rump_dns.c`/`rump_dns_pkt.c`), the x86_64 shell commands
      `ping`/`nslookup`/`wget` behind `CONFIG_TOOLS` (one-line stubs when
      `CONFIG_NET=n`, minimal links neither) whose requests the net task
      runs through `rump_toolreq.c`, the boot `dns -> ping -> wget`
      self-test (`rump_tools.c`) and the `phase_dns_tools` offline gate:
      python-stdlib UDP DNS on 127.0.0.1:5353 reachable as 10.0.2.2 plus
      the R6 HTTP fixture, with the shell commands fed over the serial
      console after the boot self-test.  It also fixed two latent bugs the
      second task exposed: the x86_64 context switch now saves/restores
      RFLAGS (IF=0 resume froze the PIT) and the R5 TCP test timeout is
      wall-clock based - R8 adds mbedTLS + HTTPS
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
- [ ] M14-8 full POSIX shell: bash over the native ABI (default sh,
      registered GPLv3 separate program shipped with its sources); the
      built-in shell keeps the rescue builtins

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
| 2026-09 | C4 kernel subsystem isolation: default `.config` absent -> `minimal` (shell + `root@Fantuan-MTF> ` prompt, zero `net:`/`rump:` and zero `tick:` lines, typed commands clean); `smoke-config.sh` PASS (minimal/net cargo-tree edge invariants, 2.7 MB budget, incrementality); `smoke-net.sh` PASS; `smoke.sh` 13/13; `smoke-bios.sh` 2/2; `smoke-riscv.sh` 3/3 (third run after the documented uart flake); three-target builds zero warnings. Includes the bootloader SFS fix (kernel loaded from the loaded-image volume instead of the first SFS) that made the SMM Secure Boot phase deterministic | PASS |
| 2026-09 | C2 catalog/appctl: `tools/smoke-apps.sh` PASS (fixture add/sync + lock sha256, kernel-layer GPL refusal plus apps-layer allow list, menu fragment merged by `tools/kconfig.py --check`, SBOM JSON, remove cleanup, corrupt-tree failure); `bash -n tools/{smoke-apps,mkbranches}.sh` clean; three-target builds zero warnings | PASS |
| 2026-09 | C3 bash vendor/GPL compliance: `tools/smoke-gpl.sh` PASS (tarball sha256 `0d5cd86965f8...` = `SHA256SUMS` = manifest and GPG-verified upstream; `COPYING` = the tarball's GPLv3 text; kernel/base `verify` refused with the GPL message while `--apps-layer` passed via `gpl_allow`; `menu` kept `CONFIG_APP_BASH=n` with the posix-libc/M14 note; SBOM bash `gpl=true`; the x86_64 kernel rebuild left `apps/bash` untouched, with no Cargo edge into `apps/` and no bash symbols/app paths in the ELF); `smoke-apps.sh` PASS, `smoke-config.sh` PASS; three-target builds zero warnings | PASS |
| 2026-09 | C1 config foundation: `tools/smoke-config.sh` PASS (net/minimal invariants, 2 MiB budget check with 1-byte over-budget simulation, `DEBUG_SELFTEST` flip rebuilds only the three config consumers); `smoke-net.sh` PASS, `smoke-bios.sh` 2/2, `smoke-riscv.sh` 3/3 (phase A rerun once after the documented serial-input flake); three-target builds zero warnings | PASS |
| 2026-09 (v0.0.2) | release builds zero warnings: `cargo build -p fantuan-kernel` (x86_64), `cargo build -p kernel-riscv` (riscv64), `tools/build-i686.sh` | PASS |
| 2026-09 (v0.0.2) | `tools/smoke.sh` full 13-phase x86 suite (second run; the first hit a host-load boot timeout in phase 9, the isolated rerun passed) | PASS 13/13 |
| 2026-09 (v0.0.2) | `tools/smoke-bios.sh` (phase 1 x86_64 MBR chain + phase 2 i686 PIO ATA VFS) | PASS 2/2 |
| 2026-09 (v0.0.2) | `tools/smoke-riscv.sh` (third run, after two hits of the documented `uart::log_bytes` flake) | PASS 3/3 |
| 2026-09 (v0.0.2) | `tools/smoke-iso.sh` (El Torito BIOS + OVMF 0xEF from the same ISO) | PASS 2/2 |
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
| 2026-09 | hybrid ISO BIOS boot (El Torito no-emulation) | PASS |
| 2026-09 | hybrid ISO UEFI boot (OVMF mounts the 0xEF FAT image) | PASS |
| 2026-09 | i686 VBE 1024x768x32 console (headless screendump, 45,777 lit pixels, text decoded) | PASS |
| 2026-09 | i686 framebuffer fallback under `-vga none` (serial identical) | PASS |
| 2026-09 | M11 R2 rump adaptation self-test (mbuf/pool/callout in-kernel) | PASS |
| 2026-09 | M11 R3 net_ops + lo0 127.0.0.1/8 + ping over loopback (`smoke-net.sh`) | PASS |
| 2026-09 | M11 R4 real ip_input/ip_output ping, UDP PCB exchange, ARP self-test (`smoke-net.sh`, `smoke-bios.sh` 2/2, `smoke-riscv.sh` 3/3) | PASS |
| 2026-09 | M11 R5 real socket/TCP on loopback: handshake, 64 KiB hash-checked transfer, close, drop/retransmit (`smoke-net.sh`; 3-target builds zero warnings) | PASS |
| 2026-09 | M11 R6 e1000 + DHCP lease + SLIRP HTTP fetch (`smoke-net.sh` LOOPBACK+SLIRP; `smoke-bios.sh` 2/2, `smoke-riscv.sh` 3/3; 3-target builds zero warnings) | PASS |
| 2026-09 | M11 R7 DNS resolver + `ping`/`nslookup`/`wget` + offline DNS gate (`smoke-net.sh` LOOPBACK+SLIRP+DNS/TOOLS, shell commands over serial; `smoke-config.sh`; `smoke-bios.sh` 2/2, `smoke-riscv.sh` 3/3; 3-target builds zero warnings) | PASS |
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
  after the NVRAM BootOrder line during repair); two attempts during the
  W5 checkpoint hit it again and reruns passed. At the W6/M10-7 checkpoint
  it hit once in phase C and once in phase B of two consecutive full runs
  (`stval=0x7f8 sepc=0x80200c4e` each time); the third full run passed
  3/3. Watch item; hunt in M11 follow-ups.

## M10 completion plan

The remaining M10 work (4a2 hardening, 4b3b ELF32, 4c VFS, 6 ISO,
in-scope 5 VBE, 7 docs/release) was sequenced in `M10_PLAN.md`; all six
workstreams (W1-W6) are closed and the release verification is in the
snapshot table above. **Released as 0.0.2** (workspace and banners); the
`v0.0.2` tag is local and owner-gated - this repository creates no tags.

## Cross-cutting - kernel configuration (Kconfig-lite)

Owner direction 2026-09: the kernel stays a kernel; the default build is
the shell only and everything optional (tools, network, TLS, graphics,
virtualization, desktop, bash) is selected through a `menuconfig`-style
config. Plan, schema and profiles: `CONFIG_PLAN.md`. Proposed as **R6.5**
between R6 and R7 so the R7 tools are config-gated from the start.
Additional constraints recorded there: boot+kernel+shell <= 300 MiB with
the OS layer unbounded but incrementally rebuilt; the project is the
kernel of a **Live OS** (not rescue-only); `CONFIG_SECURE_WIPE` clears
RAM on clean shutdown against cold-boot attacks; tools are add-ons that
keep GPL out of the kernel/base; the desktop scope is XFCE + CDE only.

Tool/package architecture: `APPS.md` - the catalog lives on the
`fantuan-apps` branch (vendored sources, community PRs) and the tooling
on the `package` branch; `main` integrates vendored `apps/<name>/` trees
with `apps.lock`. GitHub carries every branch (`main`, `fantuan-apps`,
`package`); Codeberg mirrors the **pure kernel only** (`main`), no catalog
or tooling branches. Batches: **C1** config foundation, **C2** catalog/
appctl, **C3** bash vendor + GPL compliance, **C4** kernel subsystem
isolation; then R7.

- [x] C1 config foundation: `config/Kconfig` (12 symbols + documented
      `CONFIG_APP_*` hook, fragments merged from `config/apps/*.kconfig`),
      `tools/kconfig.py` (menu/`--text`/`--olddefconfig`/`--symbol`/
      `--profile minimal|net|desktop|hypervisor|all`/`--check`/`--emit`),
      shared `tools/kconfig_emit.rs` so every crate's build.rs emits
      `cargo:rustc-cfg=kconfig_<lower>` + check-cfg, `kconfig_net` gating
      of `kernel/src/{main,timer,net}.rs` and `kconfig_debug_selftest` for
      the rump self-test, and `tools/smoke-config.sh` (net/minimal
       invariants, 300 MiB budget, incrementality) - verify
       `tools/smoke-config.sh`, three-target zero-warning builds.
       Default-profile caveat (closed by C4): the build scripts defaulted to
       `net` until C4 flipped the default to `minimal` and made `kernel-net`
       conditional.
- [x] C2 catalog + appctl: `apps/README.md`, `apps-catalog.toml`
      (`fantuan-apps` git source + disabled `overlay` path, `[licensing]
      gpl_allow` = the apps-layer list), empty `apps.lock`, and
      `tools/appctl/` (stdlib; `list`/`add`/`remove`/`sync`/`upgrade`/
      `verify`/`menu`/`sbom`) with sha256 tree pins, the
      `abi_min <= fantuan_abi ABI_VERSION` check, the GPL firewall
      (kernel/base refuses `gpl = true`; `--apps-layer` accepts only listed
      names), generated `config/apps/<name>.kconfig` fragments merged by
      `tools/kconfig.py`, and the offline fixture gate `tools/smoke-apps.sh`.
      Branch skeletons (gitignored) under `build/branch-skeletons/` plus the
      owner-run `tools/mkbranches.sh` for the local `fantuan-apps`/`package`
      branches (push commands printed, never run automatically) - verify
      `tools/smoke-apps.sh`, `bash -n tools/{smoke-apps,mkbranches}.sh`,
      three-target zero-warning builds. No real app vendored yet (the owner
      picks the list); bash lands in C3.
- [x] C3 bash vendor + GPL compliance: `apps/bash/` (GNU Bash 5.3, pristine
      `src/bash-5.3.tar.gz` + `.sig`/`SHA256SUMS`/`SOURCE`, `COPYING`, an
      empty `patches/` with the M14 musl/fantuan-ABI plan, `manifest.toml`
      with `gpl = true`, `tarball_sha256` and the new
      `requires = ["posix-libc"]`), `apps.lock` pin (`source = "upstream"`),
      catalog `gpl_allow = ["bash"]`, `THIRD_PARTY.md` register row plus the
      source-provision note, the `requires` gate in appctl (`menu` writes
      `default n` with the M14 unavailable note; `sync`/`upgrade` skip
      upstream pins) and `tools/smoke-gpl.sh` (tarball sha256 + COPYING
      provenance, kernel/base refusal vs apps-layer allow, requires gate,
      SBOM `gpl=true`, kernel isolation: no Cargo edge, tree untouched, no
      bash symbols/paths in the x86_64 ELF) - verify `tools/smoke-gpl.sh`,
      `tools/smoke-apps.sh`, `tools/smoke-config.sh`, three-target
       zero-warning builds. bash is shipped but not built: the musl/POSIX
       layer and the port land at M14-4/M14-8.
- [x] C4 kernel subsystem isolation (working tree; owner to commit):
      default profile flipped to `minimal` (`tools/kconfig.py`,
      `tools/kconfig_emit.rs`, `build*.sh` materialize it when `.config` is
      absent), `kernel-net` optional behind the `kconfig-net` cargo feature
      with the feature read from `.config` and lockstep cfg emission
      (`cargo tree` minimal has no edge, net does), gating extended to
      `bootrepair` (`kconfig_rescue_repair`), `diag::virt`+ACPI
      (`kconfig_virt`), `diag::gpu` (`kconfig_graphics`) and SMBIOS
      (`kconfig_smbios`, new symbol), the heartbeat now stops at
      `shell: ready` (`kernel_core::heartbeat`) with the 30 s cap kept, the
      prompt is `root@Fantuan-MTF> `, and `tools/smoke.sh` /
      `tools/smoke-riscv.sh` lost their stale tid/tick assertions. The SMM
      Secure Boot phase also needed a bootloader fix: `load_kernel` opened
      the first SimpleFileSystem handle, which with the test disk attached
      was not the boot volume; it now uses the loaded-image device handle
      (falling back to the first SFS). Phase 9's budget became 240 s for the
      TCG scan time. Verify
      `tools/smoke-config.sh`, `tools/smoke-net.sh`, `tools/smoke.sh`,
      `tools/smoke-bios.sh`, `tools/smoke-riscv.sh`, three-target
      zero-warning builds.

## Next action

**R7** (M11-6 DNS resolver + `ping`/`nslookup`/`wget`) on the C4 base: the
default build is the minimal shell, net is an explicit profile/feature.
bash stays pinned behind
`requires = ["posix-libc"]`: the M14-4 musl port and M14-8 shell flip
`app_manifest.AVAILABLE_REQUIRES` and add the in-image `/usr/src/bash`
sources bundle. Stop after each batch so the owner can push.
Housekeeping: hunt the intermittent riscv `uart::log_bytes` fault and the
i686 PIO ATA polling-to-IRQ conversion when the i686 shell work needs it.
