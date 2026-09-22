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
| v0.0.3 | M11 ARM64 + full TCP/HTTPS | `[##########] 100%` | released as 0.0.3: R1-R8 verified; C5 minimal/tools/bash-prep; R9a direct-FDT boot + R9b virtio-net/TLS (`smoke-aarch64.sh` 2/2), `smoke-net.sh` PASS, `smoke-config.sh` PASS, `smoke-bios.sh` 2/2, `smoke-riscv.sh` 3/3; aarch64 UEFI/AAVMF deferred to M14 |
| v0.0.4 | M12 disk tools + NTFS + GPU + virt detect | `[##########] 100%` | released as 0.0.4: M12-1..M12-7 verified (M12-6 QEMU-only per owner decision; the real AMD link/thermal capture is a manual follow-up) |
| v0.0.5 | M13 graphics/input + interface freeze | `[#---------] 10%` | design only |
| v0.1.0 | M14 Linux userspace + bootstrap + hypervisor V2 | `[####------] 30%` | P3 bash 5.3 runs, `sh` is bash (working tree): `tools/smoke-bash.sh` PASS, `tools/smoke-dash.sh` PASS (dash selectable); P1 `tools/smoke-posix.sh` PASS |
| v0.1.5 | M15 desktop + isolation + MinGW | `[----------] 0%` | - |
| v0.5.0/1.0.0 | M16 finalize | `[----------] 0%` | - |

To v0.0.5: roughly **55%** (M10, M11 and M12 are released; M13 has its
design done).

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
- [x] M11-7 mbedTLS port + HTTPS + TLS KATs: R8 vendored Mbed TLS 3.6.7
      into `apps/mbedtls/` (pristine tarball + SHA256SUMS/SOURCE/LICENSE,
      manifest with `requires = ["kernel-net", "posix-libc"]`, lock entry,
      README with the kernel-config/glue plan; Apache-2.0 chosen from the
      dual license, `gpl = false`).  `kernel-net/build.rs` extracts and
      compiles a 35-file subset only under `CONFIG_TLS` (TLS 1.2 client,
      ECDHE-RSA, AES-GCM, SHA-256, X.509/RSA; no libc/FS/threads/net) with a
      custom config, freestanding libc shims, PIT-fed time, a boot-seeded
      SHA-256 counter CSPRNG behind `mbedtls_hardware_poll` and allocators
      over the adapter kmem arena.  The boot runs the KATs
      (`tls: KATs ok (sha256 + aes-gcm + rsa)`) and the self-test fetches
      `https://test.fantuan:18443/` against a per-run pinned CA
      (`net: https get ok (...)`); `wget` gained `https://` + `--insecure`.
      The offline smoke (`tools/smoke-net-tls.sh`, called by
      `tools/smoke-net.sh`) runs the TLS/UDP phases: per-run CA, python-ssl
      server, shell `wget https://10.0.2.2:18443/`, and the host UDP echo
      test `net: udp host ok (tx=4 rx=4 bytes=1024)` (the UDP coverage Tor
      SOCKS cannot carry).  `tools/tor_relay.py` sniffs SNI/Host and
      forwards through Tor 9050 or I2P 4447 (SOCKS5) / 4444 (HTTP CONNECT);
      the optional external phase tested github.com (200), x.com (200),
      duckduckgo.com (200 + HTML marker) through live Tor, and recorded the
      Duck.AI round best-effort (status 200 without an x-vqd-4 token, chat
      POST answered 418 with a 75-byte body).  `tools/kconfig.py` gained the
      `tls` profile; net builds link no TLS code; `tools/smoke-gpl.sh`
      stays green
- [x] M11-8 aarch64 port bring-up; smoke phases; release: R9a brought up
      the direct FDT boot (R9a outcome in `M11_PLAN.md`); R9b (2026-09) adds
      the full stack on aarch64. `kernel-net` now builds for
      `aarch64-unknown-none` (the clang flags are per-target:
      `-mno-outline-atomics`, no x86-only flags) and `kernel-aarch64/src/net.rs`
      supplies the Env hooks (log/panic, frame-page DMA through the direct
      map, generic-timer ticks, cooperative sleep/yield, identity `mmio_map`
      with a TLB flush). The new polled virtio-net MMIO adapter
      (`rump_virtio_net*.c`, ours) drives the QEMU `virt` slots at
      0x0a000000+0x200*n: modern version-2 transport, VERSION_1 + MAC/STATUS
      negotiation, 12-byte modern headers, one 16-descriptor RX and TX
      virtqueue, RX/TX ring reclaim and the full-frame (no MRG_RXBUF) mode
      behind the same `net_ops` contract as the e1000; the shared
      `rump_ether_if` ifnet core is used by both drivers now. `rump_nic.*`
      keeps the DHCP/HTTP/tools code arch-neutral. The offline gate runs on
      aarch64 via SLIRP + `-device virtio-net-device`; the full marker set
      passes (virtio-net up, lease, IPv4/TCP, HTTP/DNS/ping/wget, TLS KATs +
      HTTPS, host UDP, external skip, shell tools). Three latent
      x86-masked bugs were fixed on the way: the aarch64 `splraise` cookie
      polarity (DAIF.I is inverted vs x86 IF), `sockaddr_dup(NULL)` (route
      keys/gateways) and the DHCP pseudo-header source (prefsrcip, else the
      UDP checksum used 0.0.0.0). The cooperative lock shim yields in
      `turnstile_block` instead of aborting when the shared lwp0 makes
      cross-task contention look recursive. `tools/smoke-aarch64.sh` now has
      the R9a + R9b phases; `smoke-config.sh` covers aarch64; the aarch64
      UEFI/AAVMF path is deferred to M14 (stable `aarch64-unknown-uefi` is
      installable and AAVMF is present, but the loader port - exact load
      address, ExitBootServices, MMU/cache-off trampoline, DTB from the FDT
      config table - is a batch of its own and the direct-FDT path is the
      supported one). Workspace/banners/locks/docs are at 0.0.3.

## v0.0.4 - M12 (tools + hardware)

Design: `M12_TOOLS_HW.md`.

- [x] M12-1 ACPI table walker: RSDP/RSDT/XSDT validation, FADT/MADT
      (enabled CPUs)/DMAR/IVRS presence — verify the x86 boot log lines
      (UEFI only; the BIOS path has no RSDP yet)
- [x] M12-2 disk imager + `clone` command + hash verification:
      `CONFIG_IMAGER` (default n; rescue/net/tls/desktop/hypervisor
      profiles), `kernel-core::imager` (1 MiB bounce buffer, pre-copy
      SHA-256 + destination re-read hash, progress every 5%, bounded
      retries, `q` cancellation) and the `clone <src> <dst> [--verify]
      [--yes]` command (plan print, hard source>destination refusal, YES
      gate + `RepairToken`, raw `blk0..blk3` handles) on the x86_64 and
      riscv64 command tables; the C AHCI probe registers every populated
      port so blk0/blk1 are two disks, and i686 gained the shared
      `blk_open`/`blk_name`/`blk_identity` seams with its read-only
      `blk_write` stub feeding the "destination is read-only on this
      build" error — verify `tools/smoke-imager.sh` PASS (plan/size/YES
      transcripts, kernel hashes = host sha256, verified round trip,
      untouched destination tail) plus the zero-warning profile builds
- [x] M12-3 bad-sector policy (`--continue`) + report file:
      `clone <src> <dst> [--quick] [--continue] [--retries N] [--verify]
      [--yes]` (plan gains `clone: policy continue=… quick=… retries=…`);
      without `--continue` the run aborts at the exact LBA before writing,
      with it chunk reads retry, isolate per sector and zero-fill recorded
      ranges (fixed 16-range ledger with totals, `kernel-core/src/imager/
      {policy,pass}.rs`); `--quick` hashes the first/last 1 MiB + 1 MiB at
      25/50/75% in all hash passes; every copy run writes a deterministic report
      to `/tmp/clone-report.txt` (tmpfs) and mirrors it to serial with a
      `verified`/`partial`/`failed` verdict (`imager/report.rs`). The AHCI
      command path now kicks the port after an error (`PxIS.TFES` + stop/
      clear/restart), without which retries wedged on the stuck `PxCI`.
      Fixture: `tools/mkdisk.py --badclusters <lba:count,…>` writes the
      pattern + a `.bad` sidecar; `tools/run.sh --imager-bad` turns each
      sector into a QEMU blkdebug `inject-error` read rule (a real AHCI
      error, no kernel test hook). Verify `tools/smoke-imager-bad.sh` PASS
      (abort transcript, zero-filled partial copy, report ranges/counts/
      hashes, quick verdict) plus `tools/smoke-imager.sh` PASS and the
      zero-warning profile builds
- [x] M12-4 NTFS boot/MFT/attribute/runlist read path (fixture):
      `CONFIG_NTFS` (default n; rescue/net/tls/desktop/hypervisor profiles)
      gates `kernel-core/src/vfs/ntfs/` — boot sector/BPB validation (OEM,
      512-byte sectors, clusters <= 4 KiB, MFT LCN/record size, cluster
      count), the `$MFT` bootstrap (record 0's `$DATA` runlist, fragmented
      runs), FILE records with update-sequence fixups
      (`record.rs`/`runlist.rs`), attributes (`$STANDARD_INFORMATION`,
      `$FILE_NAME` index keys, `$DATA` resident and non-resident), sparse
      runs read as zeroes, compression/encryption/attribute lists rejected
      with a clear `NtfsErr::text()`. Fixture: `tools/mkntfs.py` +
      `tools/mkntfs_fs.py` hand-build a deterministic 16 MiB NTFS 3.1
      volume (all standard metadata records, an `$I30` root with an INDX
      block, a small resident index, resident files, a 3-run fragmented
      file, a non-ASCII name and one deliberately corrupt FILE record);
      `tools/mkdisk.py --ntfs` puts it on the delivered disk and the
      builder output is validated by ntfs-3g (`ntfsls`/`ntfscat`).
      Verify `tools/smoke-ntfs.sh` PASS plus the zero-warning profile
      builds
- [x] M12-5 NTFS listing/read + `/mnt/win0`; probe graduation:
      `$I30` directory enumeration (`index.rs`, INDEX_ROOT + a bounded
      breadth-first INDEX_ALLOCATION walk with INDX fixups), `$DATA` reads
      through the runlist with a 2 x 4 KiB cluster cache (`file.rs`/
      `cache.rs`), path resolve/list/read on the mounted volume and the
      read-only `/mnt/win0` mount (`vfs/mod.rs` + `vfs/ntfs/api.rs`) with
      the probe label graduating to `NTFS (mounted ro)`. The shell gains
      `ls [path]` and `cat /mnt/win0/...` (full-file SHA-256 on cat) and
      the POSIX fd layer serves reads/readdir/stat under `/mnt/win0` while
      every write intent fails with `SYS_ERR_ROFS` (`vfs/ntfs_fd.rs`)
      — there is no write entry point in the NTFS reader at all. Verify
      `tools/smoke-ntfs.sh` PASS (mount/volume facts, listing equality,
      resident + fragmented hash equality, corrupt-record rejection,
      EROFS write attempt) and `tools/smoke-config.sh` PASS (minimal ELF
      free of the NTFS mount)
- [x] M12-6 AMD/PCI GPU report (identity/BAR/PCIe link/thermal): report-only
      probe behind `CONFIG_GRAPHICS` (the `rescue` profile now selects it,
      so `--profile rescue` prints the block and registers the shell `gpu`
      command; minimal/net link neither). Mechanics:
      `kernel/src/arch/x86_64/pci_probe.rs` (subsystem IDs, bounded
      capability walk, the write-1s BAR sizing probe with the original value
      restored, PCIe Link Capabilities/Status) and `kernel/src/diag/gpu.rs`
      (known-ID names — QEMU stdvga 0x1234:0x1111, Cirrus GD5446
      0x1013:0x00B8, virtio-gpu/QXL, AMD RX 500/5000/6000 plus iGPUs —, the
      aperture mapped through PHYS_OFFSET and read once, 256 MiB cap, 64-bit
      BARs included, no writes and no unbounded walks). `kernel/src/acpi.rs`
      walks FADT -> DSDT and scans for `_TZ_` (hook presence only; no AML);
      the summary line gains `dsdt=… tz=…`. Verify `tools/smoke-gpu.sh`
      PASS (std/cirrus/virtio identity + BAR lines, 64-bit aperture mapped,
      `pcie n/a`, no ACPI TZ, shell reprint) plus the `smoke.sh` main/shell
      phases and `smoke-config.sh` gating. QEMU-only acceptance (owner
      decision): QEMU display devices expose no PCIe capability and its DSDT
      has no thermal zone, so the RX 500/6000 link/thermal capture is the
      OPERATIONS §5.1 manual follow-up
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
- [~] M14-2 POSIX round 1 (mmap/brk/open/read/write/stat/getdents, tmpfs):
      P1 landed the first tranche; **P2** added the VMA list and anonymous
      mmap/munmap/mprotect (`process.rs`, `user::UserOps`) and the
      read-only `/bin` registry overlay for the embedded ELFs. Demand
      paging, COW and a kernel heap still belong to M14-1; kernel-core
      `vfs/{tmpfs,fd,posix}.rs`, `brk.rs`, `user::UserMemOps` and the
      `libc-fantuan` (MIT) proofs `tools/smoke-posix.sh` /
      `tools/smoke-dash.sh` cover the landed half
- [~] M14-3 POSIX round 2 (fork/execve/wait4, signals, futex, pipes):
      fork/execve/wait4, process groups/sessions, the signal trio and
      pipes landed in **P2** (see the P-batch section); P3 kept them
      unchanged while bash exercised them; `futex` and the job-control
      stop/continue set remain for P4
- [ ] M14-4 musl vs libc-fantuan decision + toybox + bmake (P4 triggers in
      `docs/POSIX_PLAN.md`; the P3 spike closes with 0/43 headers missing and
      102/102 probed symbols resolved against libc-fantuan, so the decision
      now turns on the P4 triggers, not on bash)
- [ ] M14-5 seed/self-host chain + `/bootstrap.sh` + reproducibility hash
- [ ] M14-6 C++ seed (clang) in the developer image
- [ ] M14-7 hypervisor V2 (VMX first, then SVM) + guest serial
- [x] M14-8 full POSIX shell: **bash 5.3 runs as the default `sh`**
      (`/bin/sh` and `/bin/bash`), registered GPLv3 separate program with
      its sources vendored; dash stays `/bin/dash` and is selectable. P3
      gate `tools/smoke-bash.sh` PASS; build recipe `tools/build-bash.sh`
      (stripped 743,312-byte artifact, byte-reproducible); the built-in
      shell keeps the rescue builtins

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
| 2026-09 | M12 released as 0.0.4: disk imager (`clone` + `--continue` bad-sector policy and report), read-only NTFS (`/mnt/win0`), the read-only AMD/PCI GPU report (QEMU-only acceptance) and virtualization V1 detection. `tools/smoke-imager.sh` PASS, `tools/smoke-imager-bad.sh` PASS, `tools/smoke-ntfs.sh` PASS, `tools/smoke-gpu.sh` PASS, `tools/smoke-config.sh` PASS, `tools/smoke-bios.sh` 2/2; x86_64 minimal/rescue/net/tls, riscv64, i686 and aarch64 builds zero warnings; workspace/banners/locks/docs at 0.0.4 | PASS |
| 2026-09 | M12-6 GPU/PCI report (working tree): `CONFIG_GRAPHICS` gates the read-only probe (`kernel/src/arch/x86_64/pci_probe.rs`: subsystem IDs, bounded capability walk, write-1s BAR sizing with restore, PCIe Link Capabilities/Status; `kernel/src/diag/gpu.rs`: known-ID names incl. QEMU stdvga 0x1234:0x1111, Cirrus GD5446 0x1013:0x00B8, AMD RX 500/5000/6000, each memory BAR mapped through PHYS_OFFSET and read once, 256 MiB cap, 64-bit BARs) and the `rescue` profile now selects it. The boot prints `  gpu: 00:02.0 1234:1111 QEMU stdvga [display] ss=1af4:1100` + `  gpu: bar0 0x80000000 size 16M (mapped ro)` + `  gpu: pcie n/a (no PCIe capability)` + `  gpu: pci display devices found: 1` + `  gpu: thermal unavailable (no ACPI TZ)`; the shell `gpu` command re-prints the same block. ACPI walks FADT->DSDT and scans `_TZ_` (hook presence only; `acpi: … dsdt=true tz=false` on QEMU). `tools/smoke-gpu.sh` PASS twice (std BAR0 16M, cirrus BAR0 32M, virtio 16K 64-bit BAR above 4 GiB mapped ro, each with identity/BAR asserts, `pcie n/a`, no TZ and the shell reprint). `smoke.sh` phases 1 and 8 PASS with the GPU assertions (`shell: autorun 10 command(s)`); under the host's LLVM-build load the two full-suite runs timed out in the unrelated phases 7 and 9, which pass on individual reruns with longer budgets (as do 10, the keyboard phase and the 11-13 SMM sequence). `smoke-config.sh` PASS (minimal has no `gpu` token, rescue links it; net stays graphics-free); `smoke-bios.sh` 2/2. Builds zero warnings: x86_64 minimal/rescue/net/tls, riscv64, i686, aarch64. QEMU-only acceptance per owner decision: QEMU display models expose no PCIe capability and its DSDT has no thermal zone, so the real AMD RX 500/6000 `pcie link`/thermal capture stays the OPERATIONS §5.1 manual follow-up | PASS |
| 2026-09 | M12-4/M12-5 NTFS read-only (working tree): `CONFIG_NTFS` gates `kernel-core/src/vfs/ntfs/` (boot/BPB + `$MFT` bootstrap, FILE records with fixups, attributes/runlists with sparse zero-fill, `$I30` INDEX_ROOT + INDEX_ALLOCATION walk, `$DATA` reads with a 2 x 4 KiB cache) and the read-only `/mnt/win0` mount; `tools/mkntfs.py` hand-builds a deterministic fixture (validated by ntfs-3g `ntfsls`/`ntfscat`) with a 3-run fragmented file, a non-ASCII name and one corrupt FILE record. `tools/smoke-ntfs.sh` PASS: `ntfs: mounted ro — label 'FANTUANNTFS', 4096 clusters of 4096 B, MFT record 1024 B at LCN 4` + `ntfs: mounted ro at /mnt/win0 (part 2)` + probe `part 2: NTFS (mounted ro)`; root/Users listing equality; kernel SHA-256 of hello.txt (`eb399ef1…`), frag.bin (`2800da22…`, 3 runs, truncated dump), résumé.txt and alice.txt equal to the host fixture hashes; `cat: NTFS: record corrupt (update sequence mismatch)` for the broken record; userland bash `ls`/`cat` over `/mnt/win0` work and `echo x > /mnt/win0/new.txt` returns `Read-only file system`; `cmp` of the rebuilt image shows the volume byte-identical after the run. Regressions: `smoke.sh` 13/13, `smoke-bios.sh` 2/2, `smoke-config.sh` PASS (minimal NTFS-free), `smoke-imager.sh` PASS, `smoke-imager-bad.sh` PASS; x86_64 minimal/rescue/net/tls, riscv64, i686, aarch64 builds zero warnings. No real Windows 10 image is available on this host, so the optional Win10 `Windows/System32` check stays an OPERATIONS manual path | PASS |
| 2026-09 | M12-3 bad-sector policy + report (working tree): `clone --continue` retries, isolates per sector, zero-fills and records bad ranges; every copy run writes `/tmp/clone-report.txt` (tmpfs) and mirrors it to serial with `clone-report:` lines (source/destination, policy, full/quick verify, source/stream/destination SHA-256, `bad-range lba=… count=… errors=… retries=…`, totals, `verdict verified/partial/failed`); `--quick` samples the first/last 1 MiB + 1 MiB at 25/50/75%. Fixture: `tools/mkdisk.py --badclusters 100:4,700:2` writes the source + `.bad` sidecar and `tools/run.sh --imager-bad` injects each sector as a real blkdebug `read_aio` error (the AHCI path now kicks `PxCI` after a failure so retries and later sectors work). `tools/smoke-imager-bad.sh` PASS: default abort at LBA 100 (nothing written), `--continue` completes with the destination SHA-256 = the host pattern-with-bad-ranges-zeroed (`06741878…`), report `bad-ranges 2 / errors 6 / retries 18 / verdict partial`, quick run `verdict verified`. `tools/smoke-imager.sh` PASS, `tools/smoke-config.sh` PASS, `tools/smoke.sh` 13/13, `tools/smoke-bios.sh` 2/2; x86_64 minimal/imager/rescue/net/tls, riscv64, i686, aarch64 builds zero warnings | PASS |
| 2026-09 | M12-2 disk imager (working tree): `CONFIG_IMAGER` + `clone` landed. `tools/smoke-imager.sh` PASS: the plan/size-gate/YES-gate transcripts, the kernel's pre-copy and destination hashes equal to the host sha256 of the pattern source, the destination prefix identical to the source, the untouched destination tail all-zero, and the read-only message linked in the rescue ELF. The AHCI C probe registers every populated port (blk0/blk1 = two SATA disks); i686 keeps its `blk_write` -1 stub, so its rescue/IMAGER build carries the read-only branch (no i686 shell yet, documented). `tools/smoke-config.sh` covers the IMAGER string gate (minimal absent, net/rescue present); profile builds zero warnings | PASS |
| 2026-09 | P3 bash (working tree): bash 5.3 (GPLv3 app layer) builds against libc-fantuan and runs as the default `sh`; dash stays selectable. Spike before -> after: missing headers 12 -> 0 of 43, probed symbols still missing 25 -> 0 of 102 (libc-fantuan provides 102/102). `tools/build-bash.sh` configures with the freestanding clang and `-nostdlib` (host glibc cannot leak), `--without-bash-malloc --disable-nls --disable-readline --enable-static-link`, replays `patches/0001-netopen-no-network-decls.patch`, links the static ELF (743,312 bytes stripped, sha256 `38ec6a3028d8...`, byte-reproducible), embeds it as `kernel/bash_program.bin`. `tools/smoke-bash.sh` PASS asserts `sh -c` = bash, `bash -c 'echo ...'`, `exit 7` (0x700), interactive prompt/echo, `$((2+3))=5`, `x=41; echo $((x+1))=42`, a function, `echo \| cat`, `>`/`<`, `$(...)`, `^C` -> 130, `exit`, the reaps, and dash selectable. Kernel fixes: ELF loader lost its 64-page table (loads through a segment page walk, ~190 pages for bash) and page-table frames are zeroed on allocation (a recycled frame's stale entries caused a #GP); `sigsetjmp` became a call-site macro. The P2 `apps/dash` lock orphan is fixed (dash pinned), so `smoke-gpl` is green again. `tools/smoke-dash.sh` PASS (dash selected explicitly), `smoke-posix.sh` PASS | PASS |
| 2026-09 | P2 dash (working tree): dash 0.5.12 (BSD-3) runs as the default `sh`. Root causes fixed: the "0x400b85 #PF" was the M4 demo's deliberate fault test, not dash; dash's `setjobctl` foreground-pgrp spin (kernel `set_tty_pgrp` handover + `killpg(0,...)`); libc base-0 `strtoull`; `readdir`'s zero `getdents` length; missing `/bin` stat/open registry (now with `/bin/ls` + `/bin/cat`); a zombie-exit deadlock with IF=0 (arch `set_irq_enable` + TSC-backed `now_ticks`). `tools/smoke-dash.sh` PASS asserts `sh -c` (exit 0/7), interactive prompt/echo/erase/`^C`(130)/exit, `$((1+2))`, `$(...)`, `echo \| cat`, `>`/`<`, `ls /tmp`, `sh FILE`, `$?` and reaps; `tools/smoke-posix.sh` PASS, `smoke.sh` 13/13, `smoke-bios.sh` 2/2, `smoke-config.sh` PASS; x86_64 minimal/net/tls, riscv64, i686, aarch64 builds zero warnings. Limits: no job-control stop, no file-backed mmap, `/bin` not enumerable; bash is P3 | PASS |
| 2026-09 | P1 POSIX/libc foundation (working tree): P1 ABI additions `SYS_OPEN 6`..`SYS_RENAME 28` (append-only; `SYS_VERSION` stays 1), kernel-core `vfs/{tmpfs,fd,posix}.rs` + `brk.rs` + `user::UserMemOps`, x86_64 `spawn_user_args` SysV stack, writable tmpfs with `/dev/{console,null}` (disk mounts stay ro); `libc-fantuan` (MIT) archive built deterministically by `tools/build-libc.sh --verify`; `user/hello.c` runs through the ELF loader under the minimal profile (`tools/smoke-posix.sh` PASS: argv printf, brk malloc, tmpfs round trip, pipe, clock, exit, reap); `smoke-config.sh` PASS (minimal ELF still free of net/rump/tls/rescue/tool strings, 2 MiB budget); `smoke-bios.sh` 2/2; x86_64 minimal/net/tls + riscv64 + i686 + aarch64 builds zero warnings; bash spike with libc-fantuan: configure exits 0 (was `cannot compute sizeof (size_t)`), missing headers 39 -> 13 of 43, 76 of 102 probed symbols defined (62 real + 14 stubs) - **bash does not run**, dash is the P2 target (`docs/POSIX_PLAN.md`) | PASS |
| 2026-09 | M11 R9b aarch64 network + TLS + 0.0.3: `kernel-net` built and run for `aarch64-unknown-none` (per-arch clang flags; `kernel-aarch64/src/net.rs` Env; polled virtio-net MMIO at 0x0a000000+0x200*n with modern negotiation, 12-byte headers, RX/TX virtqueues behind the e1000's `net_ops`); the aarch64 smoke network phase on SLIRP asserts `net: virtio-net up mac=...`, DHCP lease, the IPv4/TCP suite (64 KiB transfer + retransmit + rump self-tests), HTTP/DNS/ping/wget, `tls: KATs ok`, pinned-CA `net: https get ok`, `net: udp host ok`, `net: ext skip` and the shell `nslookup`/`ping`/`wget` transcripts; `smoke-aarch64.sh` 2/2, `smoke-net.sh` PASS (x86_64 gate unchanged), `smoke-config.sh` PASS (aarch64 rows), `smoke-bios.sh` 2/2, `smoke-riscv.sh` 3/3; x86_64 minimal/net/tls, riscv64, i686 and aarch64 minimal/net/tls builds zero warnings; version transition to 0.0.3; aarch64 UEFI/AAVMF deferred to M14 (documented blocker: the loader port, not the toolchain) | PASS |
| 2026-09 | M11 R9a aarch64 direct FDT boot: `kernel-aarch64/` raw-`Image` boot on QEMU `virt` (DTB in x0, PL011, 4K-granule TTBR0 identity + TTBR1 direct map, GICv2 + PPI 30 at 100 Hz, BRK resume, shared scheduler/heartbeat/shell); `tools/smoke-aarch64.sh` PASS; `smoke-config.sh` PASS; `smoke-bios.sh` 2/2; `smoke-riscv.sh` 3/3 (second run after the documented riscv serial-input flake dropped the first command byte); x86_64 minimal boot reaches `shell: ready`; aarch64/x86_64/i686 builds zero warnings | PASS |
| 2026-09 | C5 minimal default + tools' catalog home + bash early start: default `.config` absent -> `minimal` = SHELL only (plus the declared `BASH` symbol); minimal ELF free of net/rump/tls/rescue/tool command strings; typed `help` lists only `help`/`bootinfo`; `smoke-config.sh` PASS (net links tools without rescue commands, rescue links commands without tools, 2.6 MB budget, incrementality); `smoke-net.sh` PASS all phases; `smoke.sh` 13/13; `smoke-bios.sh` 2/2; `smoke-riscv.sh` 3/3; `smoke-iso.sh` 2/2; `smoke-gpl.sh`/`smoke-apps.sh` PASS; `build-bash-spike.sh` records configure `cannot compute sizeof (size_t)` + 39/43 headers + 102/102 POSIX symbols missing (bash does not run); x86_64 minimal/net/tls, riscv64 minimal/rescue, i686 minimal/rescue builds zero warnings | PASS |
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
| 2026-09 | M11 R8 mbedTLS 3.6.7 vendored + TLS 1.2 client + `wget https://` + boot KATs + offline TLS/UDP gate + optional external phase through live Tor (`smoke-net.sh` all phases, `smoke-gpl.sh`, `appctl verify mbedtls`; minimal/net/tls/riscv64/i686 builds zero warnings) | PASS |
| 2026-09 | x86 full suite `tools/smoke.sh` 13/13 | PASS (at v0.0.1) |

## Known issues

- **Serial interleaving can split a shell log line (flake)**: the demo
  kernel tasks and the shell both write COM1 without a shared lock
  (deliberate: the shell's `log_bytes` must not deadlock on a preempted
  holder). A `task N (tid N): hello …` line can land inside a long
  `grub-fix install`/diagnostic line, so an exact-string smoke assertion
  (e.g. `install: wrote EFI/ubuntu/grub.cfg …`) misses; rerun the suite.
  Hit once in the P2 checkpoint (phase 5) and passed on the rerun.
- **mbedTLS kernel limits (documented, M11 R8)**: certificate notBefore/
  notAfter dates are not checked because the kernel has no RTC or trusted
  wall clock (`MBEDTLS_HAVE_TIME_DATE` stays off; `mbedtls_time` returns PIT
  seconds and the pinned CA is the trust anchor); the adapter kmem arena is
  still a bump allocator, so freed mbedTLS blocks are not recycled (R2
  limit; per-run growth is small); TLS 1.3 is off (TLS 1.2 +
  ECDHE-RSA-AES128-GCM-SHA256 only); without RDRAND the boot CSPRNG seed is
  timing-derived on a deterministic VM (no long-term keys on this path).
  The external Duck.AI round is best-effort: through live Tor the status
  endpoint answered 200 without issuing an `x-vqd-4` token and the chat POST
  answered 418 with a 75-byte body, so what is proven is the POST, the
  cookie-jar path and a non-empty response, not a model answer.
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
  3/3. Watch item; hunt in M11 follow-ups. **P1 (2026-09)**: hit twice in
  three P1-tree runs (`scause=0xd stval=0x192`, UART puts loop) and the
  first-serial-byte input flake once; two pristine-HEAD worktree runs also
  failed phase A on the same input flake, confirming the smoke is flaky
  before P1.

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
isolation, **C5** minimal default + tools' catalog home + bash early port;
then R9.

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
- [x] C5 minimal default + tools' catalog home + bash early port (working
       tree; owner to commit): `RESCUE_REPAIR` defaults to n and the whole
       rescue/diagnostic command block (lsos/lsmnt/mount/umount/cat/
       diskhealth/hwdiag/lsdev/crypto-selftest/grub-fix) plus the stage-2
       storage diagnostic is gated in the command tables and the
       implementations (`kernel-core/src/shell/rescue.rs`, x86 + riscv
       tables with `CORE + RESCUE + TOOLS` lengths for every combination);
       default `minimal` = SHELL only (plus the declared `BASH` symbol, no
       code); `apps/{ping,nslookup,wget}` catalog skeletons with
       `requires = ["posix-libc"]`, `source = "planned"` lock entries and
       the interim-bridge note (APPS.md/M11_PLAN.md); bash early port
       (`apps/bash/port/{README,REQUIREMENTS}.md`,
       `tools/build-bash-spike.sh` records the configure failure + first
       missing headers/symbols, bash does not run); Live-first identity in
       USAGE/DESIGN/HANDOVER/OPERATIONS/BUILD/ROADMAP; `tools/kconfig.py`
       gained the `rescue` profile and smoke-config the minimal-ELF string
       invariants. Verify `tools/smoke-config.sh`, `tools/smoke-net.sh`,
       `tools/smoke.sh`, `tools/smoke-bios.sh`, `tools/smoke-riscv.sh`,
       `tools/smoke-iso.sh`, `tools/build-bash-spike.sh` and the
       x86_64/riscv64/i686 zero-warning builds.

## Cross-cutting - P batches (POSIX/libc foundation)

Owner direction: before the M14 process model, land a small, honest POSIX
base so dash can be ported first and bash second. Ledger and blocker lists:
`docs/POSIX_PLAN.md`; the ABI stays append-only. The built-in shell was the
default `sh` until P2 made dash run: now the `sh` command (and
`execve("/bin/sh")`) is dash, and the built-in shell is the boot console /
rescue fallback.

- [~] **P1** native ABI additions (round 1) + tmpfs + libc-fantuan + first C
      program (working tree; owner to commit):
      - `abi` gains `SYS_OPEN 6`..`SYS_RENAME 28` (append-only; `SYS_WRITE`
        3 keeps its v1 debug semantics, fd writes use `SYS_WRITE_FD` 9),
        Fantuan-native negative errnos and the `Stat`/`Timespec`/`Dirent`/
        `Termios`/`Winsize` layouts (`abi/src/posix.rs`);
      - kernel-core: `vfs/tmpfs.rs` (static tables: 32 nodes, 8 files x
        8 KiB, 64 KiB pool), `vfs/fd.rs` (16 fds/task, 64 opens, 8 pipes x
        512 B, refcounted dup/dup2), `vfs/posix.rs` (user copies + path
        syscalls), `brk.rs` (per-task heap at 0x500000),
        `user::UserMemOps` and task fd/heap reset at registration/exit;
        `/` + `/tmp` `/etc` `/bin` `/usr` writable, `/dev/{console,null}`
        fixed, disk mounts stay read-only;
      - `libc-fantuan/` (new, MIT, first-party): P1 headers plus
        `sys/time.h`, `sys/resource.h`, `sys/select.h`, `sys/times.h`,
        `sys/ioctl.h`, `poll.h`, `strings.h`, `pwd.h`, `inttypes.h`,
        `paths.h`, `sys/file.h`, `sys/param.h`; wrappers, brk malloc,
        string/memory, buffered stdio, printf core, dirent, time, termios,
        ctype, pwd/grp and ENOSYS stubs; built by `tools/build-libc.sh`
        (deterministic, `--verify`);
      - first C program: `user/hello.c` -> `kernel/hello_program.bin`,
        spawned by `spawn_user_args` with the SysV `argc/argv/envp` stack;
        `tools/smoke-posix.sh` asserts `user: hello from C (argc=1
        argv0=/bin/hello)`, the tmpfs round trip, the pipe, the clock, the
        exit and the reap under the **minimal** profile;
      - bash spike now measures progress: with libc-fantuan configure exits
        0, missing headers drop 39 -> 13 of 43, and 76 of 102 probed
        symbols are defined (62 real + 14 stubs). **bash does not run**;
        dash is next (P2).
      - smoke-riscv note: three P1-tree runs hit the documented pre-existing
        flakes (1x dropped first serial input byte, 2x the `uart` load
        fault); two pristine-HEAD worktree runs failed on the same input
        flake, so this is not a P1 regression (see Known issues).

## P2 (landed in the working tree; owner to commit)

The process layer is verified by `user/proc_test.c` (fork/wait4/mmap/
execve/pipe) and `p2: process layer ready` prints at boot. dash 0.5.12
(BSD-3-Clause, `apps/dash/`) is cross-built against libc-fantuan and
embedded; the `sh` command launches it on /dev/console and passes
arguments through. The `tools/smoke-dash.sh` gate boots the minimal
kernel and asserts `sh -c`, arithmetic/substitution, pipelines,
redirections, `ls /tmp`, scripts by file, `$?`, interactive
echo/erase/`^C`/`exit` and the reaps.

Root cause recorded in `docs/POSIX_PLAN.md` P2 outcome: the "dash #PF at
0x400b85" was the M4 Rust demo's deliberate fault test, not dash. The
true blockers were the job-control foreground-pgrp spin (kernel pgrp
handover + `killpg(0, ...)`), libc `strtoull` base-0 inference,
`readdir`'s zero `getdents` length, the missing `/bin` stat/open
registry, and a zombie-exit scheduler deadlock with IF=0 (IRQ-enable
hook + TSC-backed `now_ticks`). Also added first-party `/bin/ls` and
`/bin/cat`. Still unsupported: job-control stop/continue, file-backed
mmap, `/bin` enumeration; bash is P3.

## Next action

**M13 V-a** (M12 released as 0.0.4): `fb_info` + blit/fill/damage core with
the GOP console refactored on top (`docs/M13_GRAPHICS.md` §2, §7 M13-1).
M12 shipped and is verified by its gates: the disk imager
(`tools/smoke-imager.sh`), the bad-sector policy (`tools/smoke-imager-bad.sh`),
read-only NTFS with `/mnt/win0` (`tools/smoke-ntfs.sh`) and the report-only
GPU/PCI probe (`tools/smoke-gpu.sh`, QEMU-only acceptance; the real AMD
RX 500/6000 link/thermal capture is the OPERATIONS §5.1 manual follow-up).
**P3 is landed in the working tree and verified** (`tools/smoke-bash.sh`
PASS plus `tools/smoke-dash.sh`, `tools/smoke-posix.sh` and the regression
smokes); the next POSIX step is **P4** — the musl vs libc-fantuan decision
per `docs/POSIX_PLAN.md`, driven by its triggers (TLS/threads/in-system
toolchain), not by the shell. M14-1 (demand paging/COW/kernel heap) and
M14-5..M14-7 remain as listed; M14-8 (bash as the default `sh`) is closed.
The aarch64 UEFI/AAVMF path stays deferred to M14 (loader-port reason in
`M11_PLAN.md`); the direct-FDT path is the supported aarch64 boot. Stop
after each batch so the owner can push.
Housekeeping: hunt the intermittent riscv `uart::log_bytes` fault and the
i686 PIO ATA polling-to-IRQ conversion when the i686 shell work needs it.
