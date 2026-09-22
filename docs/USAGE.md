# Usage Guide (Live Operator)

How to boot the Live kernel and use its built-in shell; the rescue profile
adds hardware/boot-chain diagnosis and consent-gated repair. For building
it see [BUILD.md](BUILD.md); for the smoke suites and flag reference see
[OPERATIONS.md](OPERATIONS.md).

## 1. What this is

The kernel of a **Live OS** (RAM-first, clean shutdown, optional
persistence), self-written and booting on bare metal or in QEMU. The default
build is the minimal boot set — kernel + boot + shell (the declared
`CONFIG_BASH` full shell arrives with the M14 POSIX layer) — and everything
else is opt-in through profiles: `rescue` (diagnostic commands + boot
repair), `net`/`tls` (TCP/HTTPS; tools come from the app catalog at M14, with
the non-default in-kernel commands as the interim bridge until then). The
rescue path inspects storage and boot-chain problems read-only and only
writes after an explicit `YES`. The main paths:

| | x86_64 | riscv64 |
|---|---|---|
| Boot path | self-written UEFI bootloader (GOP + serial) | OpenSBI (`-bios default`) |
| Console | 16550 serial + GOP framebuffer mirror | NS16550 MMIO UART |
| Storage | AHCI (reference) and NVMe | virtio-mmio block |
| UEFI/NVRAM repair | yes (Runtime Services) | no (diagnosis only, honest degrade) |
| Shell commands | 2 core; `rescue` adds 10; `net`/`tls` add 3 tools | 2 core; `rescue` adds 7 |

Current release: **v0.0.3** (see [HANDOVER.md](HANDOVER.md) for status).
The release adds a self-written legacy-BIOS boot chain (x86_64 and i686)
that works on machines without UEFI; the i686 kernel is read-only against
disks. See §9 for the support matrix.

## 2. Quick start

```sh
tools/run.sh                                  # x86, headless serial console
tools/run.sh --graphics                       # x86, GOP window + serial
tools/run.sh --smm --shell-repair             # x86, NVRAM writes need SMM OVMF
tools/run.sh --arch riscv64 --disk --two-fs   # riscv + virtio-blk + FAT/ext4
```

Quit QEMU with `Ctrl-A X` (headless) or close the window.

## 3. What a boot looks like

x86: bootloader banner -> `handshake ok` -> memory map summary ->
hardware diagnostics (the storage stage is part of the `rescue` profile) ->
VFS mount -> userland demo tasks -> the shell prompt `root@Fantuan-MTF> `.

riscv: OpenSBI banner -> `fantuan v0.0.3 (riscv64)` -> FDT memory/CPU
report -> Sv39 tables -> `blk: virtio registered` -> VFS + read-only
boot-repair diagnosis -> scheduler and user tasks -> `root@Fantuan-MTF> `.

BIOS (x86_64): `fantuan-bios stage2 (M10-3)` -> E820 -> `handshake ok` ->
long-mode kernel -> tasks, userland, `root@Fantuan-MTF> ` on serial only (the BIOS
path has no GOP console).

BIOS (i686): `fantuan v0.0.3 (i686) - BIOS handoff` -> VBE text console
(serial mirror) or `fb: unavailable (serial console)` -> memmap, frame
allocator, IDT/PIC/PIT, scheduler, ELF32 user tasks and the read-only VFS
-> the kernel halts after the demo lines; there is no interactive shell on
i686 yet.

The shell is also fed by an autorun script when the ESP contains
`EFI/fantuan/shell.cmd` (see the `--keys` / `--shell-repair` fixtures).

## 4. Shell commands

The default minimal kernel's table holds only the **core builtins**
(`help`, `bootinfo`). The `[rescue]` rows need `CONFIG_RESCUE_REPAIR=y`
(`tools/kconfig.py --profile rescue`); the `[tools]` rows need
`CONFIG_TOOLS=y` plus networking (the `net`/`tls` profiles) and are the
non-default interim bridge until the app catalog takes over at M14. The
`clone` row needs `CONFIG_IMAGER=y` (the `rescue`/`net`/`tls`/`desktop`
profiles enable it; the minimal kernel has no imager).

| Command | Profile | What it does |
|---|---|---|
| `help` | core | lists the command table |
| `bootinfo` | core | boot handover details (memory map, framebuffer, RSDP, ...) |
| `hwdiag` | [rescue] | re-runs hardware diagnostics (x86 only) |
| `lsdev` | [rescue] | lists PCI storage/display devices + drive identity (x86 only) |
| `lsos` | [rescue] | filesystems per partition (the probe table) |
| `lsmnt` | [rescue] | current mount aliases |
| `mount esp0 /mnt/esp0` | [rescue] | read-only alias for the ESP |
| `umount <path>` | [rescue] | removes an alias |
| `ls [path]` | [rescue] | lists a directory (FAT32/ext4/NTFS; `/mnt/win0` for NTFS) |
| `cat <path>` | [rescue] | prints a file (FAT/ext4/NTFS, up to 4 KiB; NTFS also prints the full-file SHA-256) |
| `diskhealth [--scan]` | [rescue] | identity + SMART; `--scan` reads the surface (`q` cancels) |
| `grub-fix [diagnose\|repair\|install]` | [rescue] | boot-repair chain (see below) |
| `crypto-selftest` | [rescue] | SHA-256/RSA known-answer tests (x86 only) |
| `clone <src> <dst> [--quick] [--continue] [--retries N] [--verify] [--yes]` | [imager] | verified raw-sector copy of `blk0`..`blk3` (M12); bad-sector policy + report (M12-3) |
| `ping <host> [count]` | [tools] | ICMP echo (count 1-5) |
| `nslookup <name> [server]` | [tools] | DNS A-record lookup |
| `wget [--insecure] http[s]://host/` | [tools] | HTTP/HTTPS GET, status/bytes |

Examples:

```
root@Fantuan-MTF> lsos
root@Fantuan-MTF> mount esp0 /mnt/esp0
root@Fantuan-MTF> cat /mnt/esp0/EFI/fantuan/shell.cmd
root@Fantuan-MTF> diskhealth
root@Fantuan-MTF> grub-fix                 # same as diagnose: read-only report
root@Fantuan-MTF> clone blk0 blk1          # plan, then type YES; hash-verified
```

On virtio storage `diskhealth` prints
`SMART unsupported for this transport (virtio)` — no zero-filled fake
values.

## 5. The repair model (safety)

The commands below are compiled in with the `rescue` profile; the default
minimal kernel cannot repair because it does not carry the rescue commands
(`grub-fix` is not registered without `CONFIG_RESCUE_REPAIR`).

- **Read-only by default.** Every diagnostic, listing, mount and `cat` is
  read-only. The kernel enforces this with a `RepairToken`: write functions
  cannot even be called without it.
- **Two-step consent.** Writes require `grub-fix repair` (or `install`) and
  then typing `YES` at the `confirm>` prompt. Anything else aborts.
- **What repair does** (x86):
  - copies `EFI/ubuntu/shimx64.efi` to `EFI/BOOT/BOOTX64.EFI` when the
    fallback loader is missing;
  - writes and reads back `FIXED.TXT` as a self-test;
  - rebuilds the UEFI `BootOrder`, deletes stale entries and creates a
    missing ESP boot entry via `SetVariable` (needs SMM OVMF in QEMU);
  - `grub-fix install` backs up `grub.cfg`, regenerates it from the ext4
    root (`/boot` inventory + `/etc/fstab`), publishes and re-reads it.
- **On riscv** the FAT write path and fallback copy work over virtio-blk;
  the NVRAM half reports `runtime services unavailable` and is skipped.
- **On i686 (BIOS)** everything is read-only: the PIO ATA block layer has
  no write path, so `grub-fix repair`/`install` are unavailable and `cat`
  is the deepest operation. `clone` reports `destination is read-only on
  this build` and writes nothing. There are also no Runtime Services on
  BIOS, so NVRAM repair is absent on both BIOS paths.
- **`clone <src> <dst>`** copies raw block devices sector by sector
  (`blk0`..`blk3`; on x86_64 the test rig exposes one AHCI disk per SATA
  port). It prints a plan first, refuses when the source is larger than the
  destination, and writes only after `YES`. Verification is mandatory: the
  source is SHA-256-hashed before the copy and the destination is re-read
  and hashed after; only matching hashes report success (`--yes` skips the
  confirmation for scripts, `--verify` is accepted explicitly). `q` cancels
  at a chunk boundary, leaving a partially written destination that the
  output says is not verified.
- **Bad sectors.** Without `--continue` an unreadable sector aborts the run
  before anything is written (the source pre-hash fails) and names the exact
  LBA. With **`--continue`** each failed read is retried `--retries N` times
  (default 3) and then isolated sector by sector; a sector that stays
  unreadable is zero-filled at the destination, counted, and recorded as an
  LBA range, and the copy carries on. This is a **partial** copy, never
  silently reported as verified: the output says `PARTIAL — not a
  byte-for-byte source copy` and the report's verdict is `partial`. Always
  inspect the report before trusting such a destination.
- **`--quick`** verifies a sample (first + last 1 MiB plus 1 MiB at
  25/50/75%) instead of the whole range; the report marks `verify quick`.
  Use it only to make a large clone's check cheaper, never to hide errors.
- **Report.** Every `clone` copy run (past the plan and YES gates) writes a
  deterministic report to **`/tmp/clone-report.txt`** (the writable tmpfs;
  it is not on
  disk and disappears on reboot) and mirrors the same lines to the serial
  log prefixed `clone-report:`. It lists the source/destination and sizes,
  the policy, full or quick verification, the copied sector count, the
  source/stream/destination SHA-256, the bad-sector ranges
  (`lba`/`count`/`errors`/`retries`), the totals, and the final
  `verified`/`partial`/`failed`/`cancelled` verdict, so a transcript or
  report file can be grepped directly.

## 6. Typical rescue workflows

1. **Diagnose a no-boot machine** — boot the kernel, read the bootrepair
   report: ESP scan, `grub.cfg` search UUID, `/etc/fstab` cross-check,
   `/boot` inventory, recommendations.
2. **Inspect files without mounting on the host** — `mount esp0 /mnt/esp0`,
   then `cat` configs; read the ext4 root read-only with `cat` too, and a
   Windows volume read-only with `ls /mnt/win0` / `cat /mnt/win0/...`
   (NTFS is never written; there is no write path).
3. **Repair a missing fallback loader** — `grub-fix repair`, answer `YES`;
   watch for `repair: copied ... (N bytes, verified)`.
4. **Restore a boot entry** — same command: stale entries are deleted, a
   missing ESP entry is created and verified.
5. **Regenerate the GRUB configuration** — `grub-fix install`, answer
   `YES`; a `GRUBCFG.BAK` is kept.
6. **Check disk health** — `diskhealth`, and `diskhealth --scan` for a
   bounded (4 GiB cap) surface scan with live progress.
7. **Clone a failing disk** — `lsdev` shows the drives and their sizes;
   `clone blk0 blk1` prints the plan and asks for `YES`, then copies and
   hash-verifies the destination. If the source has unreadable sectors, use
   `clone blk0 blk1 --continue` to zero-fill and record them, then read the
   `partial` verdict and the bad-sector ranges in
   `/tmp/clone-report.txt`. The source can be larger than the target only if
   the target is bigger — the size gate refuses otherwise.

## 7. RISC-V notes

- Storage is virtio-mmio; attach a disk with `--disk` (the script passes
  `-global virtio-mmio.force-legacy=false` for the modern transport).
- There are no UEFI Runtime Services, so NVRAM/Secure Boot features are
  absent; `grub-fix diagnose` still checks the ESP, `grub.cfg`, `fstab`
  and the ext4 root.
- `diskhealth` reports SMART as unsupported for virtio (honest).
- Userland tasks run in U-mode with per-task Sv39 page tables; a fault in
  user mode kills and reaps the task, never the kernel.

## 8. Windows is not supported — use WinPE

fantuan-kernel diagnoses and repairs **Linux and BSD** boot chains only;
Windows/PE repair is a permanent non-goal. For a broken Windows boot use
the vendor's **WinPE** / installation media and `bootrec`/`bcdboot`; the
read-only NTFS tooling (M12-4/M12-5: `ls`/`cat` under `/mnt/win0`) never
writes to Windows volumes — the reader has no write entry point and every
write intent from the POSIX layer is rejected with EROFS.
Full rationale and commands: [WINDOWS.md](WINDOWS.md).

## 9. Support matrix (v0.0.3)

| Area | now (v0.0.3) | planned |
|---|---|---|
| Firmware / boot | UEFI (x86_64), legacy BIOS (x86_64 + i686), OpenSBI (riscv64), direct FDT (aarch64) | aarch64 UEFI/AAVMF deferred to M14 |
| Architecture | x86_64, i686 (32-bit, nightly toolchain), riscv64, aarch64 (stable) | more boards (M14+) |
| Storage | AHCI (every populated port), NVMe, virtio-mmio; i686 legacy PIO ATA (read-only, so `clone` destinations there are read-only) | verified disk imager (`clone` + `--continue` bad-sector policy and report, v0.0.4); more drivers (v0.0.4) |
| Filesystems | FAT32 (write-gated on x86_64/riscv), ext4 (ro), NTFS read-only (`/mnt/win0`, M12-4/M12-5), others probe-only; i686 read-only | NTFS per-file write (never planned), more read-only filesystems (v0.0.4+) |
| Network | NetBSD-derived IPv4/TCP on x86_64 (e1000) + aarch64 (virtio-net MMIO); DHCP/DNS/ping/wget, HTTP and pinned-CA HTTPS (mbedTLS); riscv/i686 have no NIC yet | user sockets (M14), IPv6/IPsec later |
| Graphics | serial + GOP console; VBE text console (i686, serial mirror) | framebuffer/KMS API (v0.0.5), XFCE/Qt (v0.1.5) |
| Virtualization | none | detect (v0.0.4), minimal hypervisor (v0.1.0), isolated mounting (v0.1.5) |
| Windows boot repair | not supported | not supported — use WinPE |

Boot paths and their repair capability:

| Boot path | Console | Repair capability | Known limits |
|---|---|---|---|
| x86_64 UEFI | GOP + serial | FAT + NVRAM (Runtime Services) | none for the rescue scope |
| x86_64 BIOS | serial only | FAT only (no NVRAM) | no GOP/ACPI/SMBIOS in the BIOS boot yet |
| i686 BIOS | VBE text + serial | none (read-only block layer; `clone` refuses with the read-only error) | 1 GiB direct-map cap, no PAE, no shell yet |
| riscv64 (OpenSBI) | NS16550 UART | FAT only (no NVRAM) | no UEFI, SMART unsupported on virtio, no network driver |
| aarch64 (QEMU virt, direct FDT) | PL011 UART | none (no block driver yet) | no storage/user mode/UEFI yet; network is virtio-net MMIO only |
| Hybrid ISO | as the firmware path | as the firmware path | CD-ROM only (no isohybrid/USB `dd`), 1 GiB budget |

The test evidence behind each row lives in
[OPERATIONS.md](OPERATIONS.md) §3 (boot matrix).
