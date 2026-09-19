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
CONFIG_SHELL            bool  default y    # rescue shell (kernel-core)
CONFIG_BASH             bool  default n    # full POSIX shell (M14-8)
CONFIG_TOOLS            bool  default n    # userland tool set
CONFIG_NET              bool  default n    # rump network stack (M11)
CONFIG_NET_DRIVERS      bool  depends NET  # e1000 / virtio-net
CONFIG_TLS              bool  depends NET  # mbedTLS + HTTPS (M11 R8)
CONFIG_VIRT             bool  default n    # hypervisor V2 (M14-7)
CONFIG_GRAPHICS         bool  default n    # fb_info/KMS API (M13)
CONFIG_DESKTOP          bool  depends GRAPHICS  # M15
CONFIG_RESCUE_REPAIR    bool  default y    # bootrepair paths
CONFIG_DEBUG_SELFTEST   bool  default n    # rump/scheduler self-tests
```

Profiles: `minimal` (SHELL + RESCUE_REPAIR), `net` (adds NET + TOOLS),
`desktop` (adds GRAPHICS + TOOLS), `hypervisor` (adds VIRT), `all`.

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
