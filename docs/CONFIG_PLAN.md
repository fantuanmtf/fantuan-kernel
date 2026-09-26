# Kernel configuration plan (Kconfig-lite)

Owner direction 2026-09: the kernel must stay a **kernel**, not an OS. The
default build is the smallest kernel with the shell (and later bash) only;
tools, networking, virtualization, graphics and the desktop are opt-in via
a `menuconfig`-style configuration instead of being pre-linked.

## Goals

- One `menuconfig` entry point; profiles for the common combinations.
- Nothing optional is linked unless selected (no dead weight, no
  pre-baked virtualization/graphics in the minimal kernel).
- The selected configuration is visible in the boot banner and in the
  generated config header; smokes run per profile.
- No GPL build tools: implement the configurator on top of the existing
  tooling (bmake/ninja are allowed later for the base image; GNU make is
  not). The configurator itself is small and self-written.

## Schema (first cut)

```
CONFIG_SHELL            bool  default y    # built-in kernel shell (kernel-core)
CONFIG_BASH             bool  default y    # GNU bash 5.3 as the login shell + /bin/sh
CONFIG_TOOLS            bool  default n    # interim kernel tool bridge
CONFIG_NET              bool  default n    # rump network stack (M11)
CONFIG_NET_DRIVERS      bool  depends NET  # e1000 / virtio-net
CONFIG_TLS              bool  depends NET  # mbedTLS + HTTPS (M11 R8)
CONFIG_VIRT             bool  default n    # hypervisor V2 (M14-7)
CONFIG_GRAPHICS         bool  default n    # fb_info/KMS API (M13)
CONFIG_DESKTOP          bool  depends GRAPHICS  # M15
CONFIG_RESCUE_REPAIR    bool  default n    # rescue/diagnostic + bootrepair (C5)
CONFIG_IMAGER           bool  default n    # disk imager + `clone` (M12-2)
CONFIG_NTFS             bool  default n    # NTFS read-only + /mnt/win0 (M12-4/M12-5)
CONFIG_SMBIOS           bool  default n    # SMBIOS identity/DIMM/slots
CONFIG_DEBUG_SELFTEST   bool  default n    # rump/scheduler self-tests
CONFIG_SECURE_WIPE      bool  default n    # Live shutdown RAM wipe
```

Profiles (C5): `minimal` (SHELL only - the Live boot set), `rescue` (adds
the diagnostic commands and boot repair; also `GRAPHICS` for the M12-6 GPU
report), `net` (adds TOOLS + NET + drivers + self-test), `tls` (net +
mbedTLS), `desktop`, `hypervisor`, `all`.
The non-default `TOOLS` bridge is the R7 in-kernel ping/nslookup/wget; the
catalog home is `apps/{ping,nslookup,wget}` from M14 (APPS.md). M12-2 adds
`IMAGER` (the verified `clone` raw-disk copy) to the rescue/tools profiles
(rescue, net, tls, desktop, hypervisor), plus a standalone `imager` profile
(SHELL + IMAGER) for gating tests; `minimal` stays imager-free.

## Mechanics

1. `config/Kconfig`-style TOML or text schema (symbol, type, depends,
   default, prompt) read by `tools/kconfig.py` (Python stdlib, menu UI
   with a fallback `--text`/`--olddefconfig`/`--profile` mode).
2. Output `.config` at the repo root plus a generated
   `build/config/features.rs` with `pub const CONFIG_*` values.
3. Crates consume it through their `build.rs`: emit `cargo:rustc-cfg=`
   for each `y` bool (e.g. `cfg(kconfig_net)`) and/or Cargo features
   wired to the same names. Optional crates (`kernel-net`) are only
   added as dependencies when the feature is on.
4. Boot banner prints the profile and the enabled feature list; smokes
   assert the minimal profile has no `net:`/`rump:` lines and the net
   profile does.
5. CI matrix: `minimal`, `net`, `all` (graphics/hypervisor land as their
   code arrives). `tools/smoke-config.sh` builds each profile and checks
   the presence/absence invariants.

## Implemented in C1 (2026-09)

The foundation is in: `config/Kconfig` (12 bool symbols, `depends on`,
prompt/help and the documented `CONFIG_APP_*` hook - fragments land in
`config/apps/*.kconfig` and are merged when present), `tools/kconfig.py`,
`tools/kconfig_emit.rs` (shared build-script helper) and
`tools/smoke-config.sh`.

```sh
tools/kconfig.py --profile net        # minimal|rescue|net|tls|desktop|hypervisor|all
tools/kconfig.py --text               # show the effective configuration
tools/kconfig.py --symbol NET=N       # flip a symbol (repeatable)
tools/kconfig.py --check              # validate depends / reject impossible
tools/kconfig.py --olddefconfig       # fill new symbols with defaults
tools/kconfig.py --emit               # build/config/features.{rs,env}, hashed
tools/smoke-config.sh                 # profile invariants + budget + increment
```

Every crate's `build.rs` (kernel, kernel-riscv, kernel-i686, kernel-core,
kernel-net) calls the shared `tools/kconfig_emit.rs` helper: it reads
`.config` and emits `cargo:rustc-cfg=kconfig_<lower>` plus the matching
`cargo:rustc-check-cfg` for every symbol, so builds stay zero-warning on
all targets. Gated end-to-end: `kernel/src/main.rs`, `kernel/src/timer.rs`
and `kernel/src/net.rs` behind `kconfig_net`, and the rump self-test task
behind `kconfig_debug_selftest`.

Default profile (C4; contents trimmed in C5): a missing `.config` resolves
to the `minimal` profile (SHELL only) in `tools/kconfig.py`, the
`tools/kconfig_emit.rs` default table and the build scripts. Everything else
is explicit: `tools/smoke-net.sh` writes `--profile net`, the riscv/BIOS
smokes write `--profile rescue`, `tools/smoke.sh` starts from minimal and
flips `RESCUE_REPAIR`/`VIRT`/`SMBIOS`, and the build scripts pass
`--features kconfig-net` only when `CONFIG_NET=y`.

## Implemented in C4 (2026-09)

`kernel-net` is now an optional Cargo dependency of the x86_64 kernel
behind the `kconfig-net` feature: no `.config`/feature means no dependency
edge (`cargo tree -p fantuan-kernel` has no `kernel-net`), and
`--features kconfig-net` adds it. `tools/kconfig_emit.rs` only emits
`cfg(kconfig_net)` when that feature is active, so the cfg and the
dependency edge can never disagree (a direct `cargo build` stays minimal).
`tools/build.sh` and `tools/build-bios.sh` derive the feature from
`.config`.

Gated subsystems (C4), all zero-warning on the three targets:

- `kernel-net` + `kernel/src/{net.rs,main.rs,timer.rs}` behind
  `kconfig_net` (C1, kept).
- `kernel-core::bootrepair` and the x86 glue, boot diagnosis, the
  `grub-fix` shell paths and the authenticated-variable bridge behind
  `kconfig_rescue_repair` (C5 extends the gate to the whole rescue command
  table, so no `grub-fix`/`diskhealth` command exists when it is off).
- `kernel/src/diag/virt.rs` + `kernel/src/acpi.rs` behind `kconfig_virt`
  (the IOMMU walk is ACPI's only current consumer).
- `kernel/src/diag/gpu.rs` behind `kconfig_graphics`; its SMBIOS slot
  input degrades to "no slots" when `CONFIG_SMBIOS=n`.
- `kernel/src/smbios/` behind `kconfig_smbios` (new symbol, default n).
- The rump self-test task stays behind `kconfig_debug_selftest` (C1).

Still ungated and why: the VFS/storage stack (`vfs`, `drivers`, AHCI/NVMe/
ATA), `mm`/`paging`/`task` (the kernel cannot boot or mount anything without
them), `bootinfo` (a core builtin that reports the handover) and the shared
shell itself (`SHELL`, enabled in every profile; gating it would compile a
console-less kernel). `BASH`/`DESKTOP`/`SECURE_WIPE` still have no code to
gate. `TLS` was in that group until R8; it now gates real code (below).

Gated in R7: `CONFIG_TOOLS` selects the x86_64 `ping`/`nslookup`/`wget`
shell commands (`kernel/src/shell/cmds_net.rs`); the clients themselves
live in `kernel-net` behind `CONFIG_NET`, and a TOOLS-without-NET build
gets one-line `not built (CONFIG_NET=n)` stubs in the `help` table.  The
minimal profile (`TOOLS=n`) links neither the commands nor the clients.

Gated in M12-2: `CONFIG_IMAGER` selects `kernel-core::imager` (the 1 MiB
bounce buffer, SHA-256 round-trip verification and the cancellation/retry
loop) and the `clone` shell command in `kernel-core::shell::imager`,
registered by the x86_64 and riscv64 command tables. It is independent of
`RESCUE_REPAIR`; the rescue/net/tls/desktop/hypervisor profiles enable it,
and the minimal kernel ELF carries neither the command nor the imager
strings.

Gated in M12-4/M12-5: `CONFIG_NTFS` selects `kernel-core::vfs::ntfs`
(boot/$MFT/attribute/runlist parsing, `$I30` indexes, `$DATA` reads, the
read cache) plus the `/mnt/win0` mount and the POSIX read-only view. The
`ls` command is part of the rescue table, and its NTFS path exists only
with the symbol. The minimal profile stays NTFS-free: `smoke-config.sh`
asserts the minimal ELF has no `win0` string and no `ntfs:` boot line,
while the rescue/net/tls/desktop/hypervisor profiles mount the fixture
read-only (`tools/smoke-ntfs.sh`).

Gated in M12-6: `CONFIG_GRAPHICS` selects the read-only GPU/PCI report
(`kernel/src/arch/x86_64/pci_probe.rs`, `kernel/src/diag/gpu.rs`) that the
boot stage prints and the rescue shell re-prints as `gpu`. The `rescue`
profile selects `GRAPHICS` (the minimal/net profiles do not): `smoke-config.sh`
asserts the minimal ELF has no `gpu` token while the rescue profile links
it. The probe is read-only (config-space reads plus the standard write-1s
BAR sizing with the original value restored, and a bounded MMIO aperture
read); the ACPI `_TZ_` scan is hook-presence only.

Gated in M13-1: the same `CONFIG_GRAPHICS` symbol now also selects
`kernel-core::graphics` (the `FbInfo`/fill/blit/damage/present core) and
gates the x86_64 GOP console (`kernel/src/{console,font}.rs`) and the i686
VBE console (`kernel-i686/src/fb.rs`) that sit on top of it, plus the
console mirror in the serial driver. The `minimal` and `net` profiles stay
graphics-free: with `CONFIG_GRAPHICS=n` the x86_64 kernel prints
`console: none (serial-only; GOP unavailable)`, the i686 kernel prints
`fb: unavailable (serial console)` and neither links the framebuffer core,
the console or the font (riscv/aarch64 are serial-only as before). The
`rescue` profile (and `smoke.sh`, which flips `GRAPHICS=Y`) keeps the full
framebuffer console and the boot `graphics: damage self-test ok` line.

Gated in M13-2: the double buffer and the kernel demo app are part of the
same `CONFIG_GRAPHICS` seam — no new symbol. The back buffer is a
frame-allocator allocation (bounded to the mode's exact byte size, e.g.
~4 MiB for 1280x800 Bpp32), not a `.bss` static, so it costs no boot image
space and the i686 `510 MiB usable` report is unchanged; when the allocator
cannot spare the buffer the console keeps `DirectPresent` (damage-tracked
single-buffered) and the demo still runs. The demo task
(`kernel/src/gfx_demo.rs`, `kernel-i686/src/gfx_demo.rs`) is
`#[cfg(kconfig_graphics)]` and only spawns when a console exists, so the
`minimal`/`net` profiles link no demo or back-buffer code and print no
`graphics:` marker; riscv/aarch64 are serial-only and never spawn it.

Gated in M13-3: the input event ring (`kernel-core::input_ring`) and the PS/2
mouse decoder (`kernel-core::mouse`) share the same `CONFIG_GRAPHICS` seam —
still no new symbol. The arch mouse drivers (`kernel/src/mouse.rs`,
`kernel-i686/src/mouse.rs`) and the keyboard→key-event routing are
`#[cfg(kconfig_graphics)]`, so the `minimal`/`net` profiles link neither the
ring, the decoder nor the mouse and carry no `input:`/`PS/2 aux` marker;
`tools/smoke-config.sh` asserts the minimal ELF is free of the `ring
self-test` and `PS/2 aux` strings. The existing keyboard ASCII path into the
serial shell stays ungated and unchanged; riscv/aarch64 have no PS/2 and keep
their serial-only input.

## Implemented in C5 (2026-09)

The default `minimal` profile is now the Live boot set only: SHELL (plus the
declared `BASH` symbol, no code yet). `RESCUE_REPAIR` defaults to n and the
whole rescue/diagnostic block is gated at the **command table**, not just in
the implementations:

- `kernel-core/src/shell/rescue.rs` (new: lsos, lsmnt, mount, umount,
  diskhealth, grub-fix) and `kernel-core/src/shell/cat.rs` are
  `#[cfg(kconfig_rescue_repair)]`; `kernel-core/src/shell/cmds.rs` keeps the
  two core builtins (help, bootinfo).
- the x86 and riscv tables assemble their length from cfg blocks
  (`CORE_COMMANDS + RESCUE_COMMANDS + TOOL_COMMANDS`), correct for every
  `RESCUE_REPAIR x TOOLS x NET` combination (TOOLS-without-NET still gets the
  one-line stubs).
- `diag::storage`/`diag::diskhealth` and the stage-2 storage report are
  gated too, so the minimal ELF carries no `diskhealth` symbols or strings.
- `tools/smoke-config.sh` proves it: the minimal ELF has zero
  net/rump/tls/rescue/tool command strings, the boot types `help` and lists
  only `help`/`bootinfo`, the net profile links the tools but no rescue
  commands, and the rescue profile links the rescue commands but no tools.

The tools themselves stay (R7 evidence) as a non-default interim bridge:
`apps/{ping,nslookup,wget}` are catalog skeletons (`requires =
["posix-libc"]`, `source = "planned"`) and take over at M14-4 (APPS.md).
Bash's early port started in `apps/bash/port/` with
`tools/build-bash-spike.sh` recording the blocker list; **bash 5.3 now runs
and is the default `sh`** (P3), built and embedded by `tools/build.sh` when
`CONFIG_BASH=y` (M14_LINUXUSERS.md). The two symbols are distinct and
deliberately unconnected: `CONFIG_BASH` drives the kernel build (fetch,
build, embed bash as `/bin/sh`), while the generated `CONFIG_APP_BASH`
marks the app-layer catalog entry, which stays `default n` behind its
`requires = ["posix-libc"]` gate until the M14 POSIX layer closes it.

Gated in R8: `CONFIG_TLS` (depends on `NET`) selects the mbedTLS subset and
the whole HTTPS path. `kernel-net/build.rs` extracts the vendored tarball
(`apps/mbedtls/src/mbedtls-3.6.7.tar.bz2`) and compiles 35 upstream files
plus the platform glue only when the symbol is set; the adapter C files see
`-DFANTUAN_TLS=1`, so `rump_tls*`/`rump_ext.c` and the `wget https://` shell
branch do not exist in net builds. `rump_r8.c` (the boot sequence for the
HTTPS/UDP/external checks) is compiled in every net build but contains no
TLS code without the flag. `tools/kconfig.py --profile tls` is the net
profile plus `TLS`. The offline smoke's fixture switch
(`FANTUAN_NET_FIXTURES=1`) and the pinned CA (`build/smoke-net-tls/ca.der`)
are consumed by `kernel-net/build.rs` and change only what the boot
self-test runs — never what is compiled or linked.

## Implemented in R9a (2026-09)

`kernel-aarch64/build.rs` consumes the same `tools/kconfig_emit.rs` helper,
so the aarch64 kernel emits `cfg(kconfig_<lower>)` + `cargo:rustc-check-cfg`
exactly like x86_64/riscv/i686. Its command table mirrors the riscv one
(`CORE_COMMANDS + RESCUE_COMMANDS` with the individual `#[cfg]` entries),
which preserves the C5 invariant: the default minimal profile links only
`help`/`bootinfo`, and the rescue commands exist only under
`CONFIG_RESCUE_REPAIR`. No new symbols are introduced: R9a brings up the
existing subsystems on the new arch, and the aarch64 NET/TOOLS/TLS gates
arrive with the R9b port.

Heartbeat: the timers stop the 10 s `tick:` line as soon as the shell is
ready (`kernel-core::heartbeat`, raised before the `shell: ready` line);
the 30 s cap stays as the no-shell fallback (i686 has no periodic heartbeat
— its PIT only drives the scheduler — so it is unchanged; aarch64 reuses
the shared heartbeat like riscv). The default prompt is
`root@Fantuan-MTF> ` (`kernel_core::shell::HOSTNAME`).

## Budgets, incrementality and the Live direction

Owner direction 2026-09, additional constraints:

- **Size budget**: boot + kernel + shell together stay within **300 MiB**
  (hard check in the build, like the ISO budget). The OS layer the user
  configures on top is unbounded.
- **Incremental builds**: changing the configuration must not rebuild the
  world. The generated config header is content-hashed; each crate gets
  `rustc-cfg`s/features per symbol, optional crates are dependency-gated,
  and the configurator offers `--olddefconfig` so only touched crates
  rebuild. Documented and smoke-checked (touch one symbol, assert the
  rebuild set is minimal).
- **Live kernel goal**: the project is not a rescue-only system. It is
  becoming the kernel for a Live environment (RAM-first operation, clean
  boot/shutdown, no persistent writes by default, optional persistence).
  Rescue stays a profile, not the identity.
- **Shutdown RAM wipe**: a `CONFIG_SECURE_WIPE` (default y in the Live
  profile) overwrites kernel memory, page caches, framebuffer and free
  RAM on clean shutdown/reboot as a mitigation against cold-boot (RAM
  freezing) attacks. Honest limits documented: a crash or power loss
  cannot be wiped, so suspend/hibernation stay disabled and the wipe is
  best-effort defence-in-depth, not a guarantee.
- **Tools as add-ons (licence firewall)**: userland tools and any GPL
  programs ship as separate, configuration-selected programs; nothing
  GPL is ever linked into the kernel or base. The default kernel stays
  BSD/MIT/Apache only, which the tools separation is designed to keep.
  The catalog, vendoring policy and branch model are in `APPS.md`; bash
  is the registered GPLv3 default shell shipped with its sources.
- **Desktop scope**: only **XFCE** and **CDE** are migration targets for
  now - small, cross-platform codebases that are tractable to port; other
  desktops are explicitly out of scope until further notice.

## Placement in the roadmap

Propose **R6.5**: land the configurator, the `.config` plumbing, the
first symbols (SHELL/TOOLS/NET/DEBUG_SELFTEST) and gate `kernel-net` plus
the rump self-test behind it, before R7 builds the ping/nslookup/wget
tools (they are `CONFIG_TOOLS`/`CONFIG_NET` from day one). Later batches
extend the schema as TLS/graphics/virt code arrives; M13 freezes the
graphics symbols together with its API.

Owner decision needed: start R6.5 before R7 (recommended) or fold the
config work into R7.
