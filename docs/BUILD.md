# Build Guide

How to build fantuan-kernel v0.0.4 (x86_64, i686, riscv64 and aarch64) from source.
For *running* what you built see [USAGE.md](USAGE.md); for the CI-style
checks see [OPERATIONS.md](OPERATIONS.md).

## 1. Toolchain requirements

| Tool | Purpose | Notes |
|---|---|---|
| Rust stable + `rustup` | kernels, bootloader, userland | targets below |
| `x86_64-unknown-none` | x86_64 kernel, userland | add target |
| `x86_64-unknown-uefi` | UEFI bootloader | add target |
| `riscv64gc-unknown-none-elf` | RISC-V kernel, userland | add target |
| `aarch64-unknown-none` | aarch64 kernel (R9a) | built-in stable target, no build-std needed |
| nightly + `rust-src` | i686 (32-bit) kernel | `rustup toolchain install nightly --profile minimal --component rust-src`; the crate pins nightly via `kernel-i686/rust-toolchain.toml` and builds core from source with `-Z build-std=core -Z json-target-spec --target targets/i686-fantuan-none.json` |
| `qemu-system-x86_64` + OVMF (`edk2-ovmf`) | run/test x86_64 | SMM OVMF optional |
| `qemu-system-riscv64` (>= 9) | run/test riscv64 | OpenSBI `fw_dynamic` ships with QEMU |
| `qemu-system-aarch64` | run/test aarch64 | QEMU `virt`; the smoke pins `gic-version=2` |
| `clang` + `llvm-ar` + `llvm-ranlib` | build the C driver layer for riscv64 and the embedded shell | any recent LLVM |
| `bison` | GNU bash's `parse.y` (only when `CONFIG_BASH=y`, the default) | `tools/kconfig.py --symbol BASH=N` builds without it |
| network (first build only) | fetches the pinned bash 5.3 tarball into `build/cache/` | `tools/fetch-bash-src.sh`; `FANTUAN_OFFLINE=1` or a pre-seeded cache/binaries removes the need |
| `llvm-objcopy` (or `aarch64-linux-gnu-objcopy`) | flatten the aarch64 ELF to the raw `Image` QEMU boots | LLVM tooling |
| `python3` | disk fixtures (`tools/mkdisk.py`) | 3.8+ |
| `objcopy` (binutils) | x86 kernel ELF -> flat binary | host binutils is fine |

Install the Rust targets once:

```sh
rustup target add x86_64-unknown-uefi x86_64-unknown-none riscv64gc-unknown-none-elf aarch64-unknown-none
```

The system `cargo` may lack bare-metal targets; the scripts export
`$HOME/.cargo/bin` themselves, so use `tools/*.sh` or export it in your
shell (`export PATH="$HOME/.cargo/bin:$PATH"`).

## 2. Workspace layout

```
abi/           fantuan-abi    BootInfo ABI + syscall numbers + PHYS_OFFSET
boot/          fantuan-boot   UEFI bootloader (x86_64-unknown-uefi)
boot-bios/     stage1 (MBR) + stage2 (E820/VBE/long mode or i686)
kernel/        fantuan-kernel x86_64 kernel (x86_64-unknown-none)
kernel-i686/   kernel-i686    32-bit kernel (nightly + custom target JSON)
kernel-core/   kernel-core    portable half shared by all three kernels
kernel-riscv/  kernel-riscv   riscv64 kernel (riscv64gc-unknown-none-elf)
kernel-aarch64/ kernel-aarch64 aarch64 kernel (aarch64-unknown-none, direct FDT)
user/          fantuan-user   the userland test program (all arches)
drivers/c/     C driver layer: blk.c + blk_ops + ahci/nvme/i8042/virtio_mmio
tools/         build/run/smoke scripts + mkdisk.py + mkiso.py
docs/          DESIGN.md (authoritative), milestone/award docs, this set
Makefile       make front end for the scripts above (build/run/smoke/clean)
config.mk      make knobs (tracked defaults; config.local.mk overrides them)
```

All crates are version `0.0.4` (`panic = "abort"`, release `opt-level = "z"`).

## 3. Build (the recommended paths)

### 3.1 make — the front door

The tree mixes Rust, C, NASM, Python and shell, and the entry point depends on
what you want (a kernel for one arch, a BIOS image, a smoke suite). `make`
wraps all of it so a fresh clone does not have to guess:

```sh
make help                  # every target, grouped, with the knobs in effect
make doctor                # toolchain check (Rust targets, QEMU, OVMF, nasm, clang, python3)
make build                 # ARCH (default x86_64), writes PROFILE first if you set one
make build-riscv           # = tools/build.sh --arch riscv64
make build-aarch64         # = tools/build.sh --arch aarch64
make build-i686 build-bios iso   # 32-bit, BIOS chain, hybrid ISO (or: make build-all)
make run                   # boot the built image; make run-graphics, run-riscv, ...
make check                 # compile check of every stable-target crate (~10 s, no QEMU)
make check-i686            # the 32-bit build (nightly + build-std, slower)
make check-lines            # the 300-line rule ratchet (tools/line-budget.txt)
make smoke                 # the quick suites: config gpl apps posix
make smoke-kms             # any single suite, by name
make test                  # the full x86 acceptance suite (13 phases, ~25 min)
make apps                  # the vendored catalog; make sbom for licences
make clean / distclean     # remove cargo targets (+ payloads) / also build/
```

`make` implements no build step of its own — every target calls the script
that owns it, so this file and the scripts cannot drift apart. It works with
GNU make and bmake.

**Two different configuration files, do not confuse them:**

| File | What it configures | Edited by |
|---|---|---|
| `config.mk` + `config.local.mk` | the *build orchestration*: `ARCH`, `PROFILE`, `RUN_ARGS`, `BUILD_ARGS`, the smoke sets | `config.local.mk` is yours (gitignored); `config.mk` is the tracked default |
| `.config` (generated, ignored) | the *kernel* itself: the `CONFIG_*` symbols — see §3.3 | `make config` / `tools/kconfig.py` |

Environment variables the scripts read (`FANTUAN_OFFLINE`,
`FANTUAN_BUILD_BASH`, `FANTUAN_BASH_TARBALL`, …) pass straight through:
`FANTUAN_OFFLINE=1 make build`.

### 3.2 the scripts behind it

What the make targets call (useful when you want one step at a time, or are
scripting something make does not cover):

```sh
tools/build.sh                 # x86: userland -> kernel -> bootloader
tools/build.sh --arch riscv64  # riscv: userland -> kernel-riscv
tools/build.sh --arch aarch64  # aarch64: kernel-aarch64 -> raw Image (llvm-objcopy)
tools/build-i686.sh            # i686: userland (ELF32) -> 32-bit kernel
tools/build-bios.sh            # BIOS image from the x86_64 kernel
tools/build-iso.sh             # hybrid BIOS+UEFI ISO (build/fantuan.iso)
```

Artifacts:

| Artifact | Path |
|---|---|
| x86 userland (embedded into the kernel by `kernel/build.rs`) | `kernel/user_program.bin` |
| x86 kernel ELF / flat binary (copied to the ESP by `run.sh`) | `target/x86_64-unknown-none/release/fantuan-kernel` |
| UEFI bootloader | `target/x86_64-unknown-uefi/release/fantuan-boot.efi` |
| riscv userland (embedded by `kernel-riscv/build.rs`) | `kernel-riscv/user_program.bin` |
| riscv kernel (loaded by OpenSBI at `0x80200000`) | `target/riscv64gc-unknown-none-elf/release/kernel-riscv` |
| aarch64 kernel ELF | `target/aarch64-unknown-none/release/kernel-aarch64` |
| aarch64 raw `Image` (QEMU loads it at `0x40080000`, DTB in x0) | `build/kernel-aarch64.bin` |
| i686 kernel flat binary (BIOS stage2 loads it) | `build/kernel-i686.bin` |
| BIOS boot image / hybrid ISO | `build/bios.img`, `build/bios-i686.img`, `build/fantuan.iso` |

Both `user_program.bin` files are generated and git-ignored; never edit them.

### 3.3 Configuration profiles (Kconfig-lite)

The kernel is configured before it is built; a missing `.config` resolves to
the default **minimal** profile (kernel + boot + shell, where `CONFIG_BASH=y`
makes the shell GNU bash 5.3 — built from the fetched sources and embedded as
`/bin/sh` by `tools/build.sh`). Everything else is opt-in:

```sh
tools/kconfig.py --profile minimal     # default: SHELL only
tools/kconfig.py --profile rescue      # + diagnostic commands + boot repair
tools/kconfig.py --profile net         # + TOOLS + rump network + drivers + self-test
tools/kconfig.py --profile tls         # net + mbedTLS/HTTPS
tools/kconfig.py --profile desktop|hypervisor|all
tools/kconfig.py --text                # effective configuration
tools/kconfig.py --check --emit        # validate + regenerate build/config/features.*
tools/smoke-config.sh                  # profile invariants, string gating, budget
```

`tools/build.sh` / `build-bios.sh` / `build-i686.sh` materialize the minimal
profile when `.config` is absent and pass `--features kconfig-net` only when
`CONFIG_NET=y`. The `.config` is read by every crate's `build.rs` through
`tools/kconfig_emit.rs`; use `make config-text` to see the effective
configuration — the boot banner does not print it. Apps add `CONFIG_APP_*`
symbols via `tools/appctl/appctl.py menu` (see [APPS.md](APPS.md)). Note the
two distinct symbols: `CONFIG_BASH` (kernel build: fetch, build and embed bash as `/bin/sh`,
`default y`) and the generated `CONFIG_APP_BASH` (app-catalog marker, still
`default n` behind its `requires = ["posix-libc"]` gate). The early-port
record stays in `apps/bash/port/`; the sources live on the `fantuan-apps`
branch and are fetched by `tools/fetch-bash-src.sh`.

## 4. Build with cargo directly

```sh
# x86 chain (order matters: userland first, then kernel, then bootloader)
cargo build -p fantuan-user   --target x86_64-unknown-none --release
cp target/x86_64-unknown-none/release/fantuan-user kernel/user_program.bin
cargo build -p fantuan-kernel --target x86_64-unknown-none --release
cargo build -p fantuan-boot   --target x86_64-unknown-uefi --release

# riscv chain
cargo build -p fantuan-user    --target riscv64gc-unknown-none-elf --release
cp target/riscv64gc-unknown-none-elf/release/fantuan-user kernel-riscv/user_program.bin
cargo build -p kernel-riscv    --target riscv64gc-unknown-none-elf --release

# aarch64 chain (R9a: no userland; flatten for QEMU's raw-Image boot)
cargo build -p kernel-aarch64  --target aarch64-unknown-none --release
llvm-objcopy -O binary target/aarch64-unknown-none/release/kernel-aarch64 build/kernel-aarch64.bin

# library-only checks
cargo build -p kernel-core     --target x86_64-unknown-none --release
cargo build -p kernel-core     --target riscv64gc-unknown-none-elf --release
cargo build -p kernel-core     --target aarch64-unknown-none --release

# i686 chain (nightly crate; wraps build-std + the custom target JSON)
./tools/build-i686.sh

# P2/P3 userland/POSIX chain (optional; `tools/build.sh` runs it by default).
# Order matters: libc archive first, then the shells (they link the archive).
tools/fetch-bash-src.sh  # bash sources are not tracked on main; verify + cache
tools/build-libc.sh    # libc-fantuan.a + hello/proc-test/ls/cat ELFs
tools/build-dash.sh    # dash 0.5.12 -> kernel/dash_program.bin (/bin/dash)
tools/build-bash.sh    # bash 5.3 -> kernel/bash_program.bin + .sha256 marker
                       # BASH_VERIFY=1 rebuilds and compares the hash
```

A bare `cargo build` of the kernel still works without those artifacts: it
only warns, and `/bin/sh` falls back to dash. `tools/build.sh` is the strict
path — it exports `FANTUAN_REQUIRE_BASH=1`, so a missing shell is an error
there, and it is what makes `/bin/sh` resolve to bash in the shipped image.

A clean build must produce **zero warnings** on all targets; treat a new
warning as a build break.

## 5. What the build scripts generate

- `kernel/build.rs`: `asm_defs.inc` from `kernel/src/consts.rs`, the
  256-entry ISR stub table, the embedded user ELF, and compiles the x86 C
  driver layer + assembly stubs with `cc` (`-mcmodel=large`,
  `-mno-red-zone`). It also embeds the optional program payloads
  (`kernel/{hello,proc_test,dash,bash,ls,cat}_program.bin`); `tools/build.sh`
  produces `bash_program.bin` and its `bash_program.sha256` marker by default
  (see §3.3), and exports `FANTUAN_REQUIRE_BASH=1` so a missing artifact is a
  build error rather than a silent fallback.
- `kernel-riscv/build.rs`: embeds the riscv user ELF and compiles
  `drivers/c/blk.c` + `drivers/c/virtio_mmio.c` with **clang**
  (`--target=riscv64-unknown-none-elf -march=rv64gc -mabi=lp64d
  -mcmodel=medany`, archived with `llvm-ar`).
- `kernel-aarch64/build.rs`: only the Kconfig cfg emission and
  `kernel-aarch64/link.ld` (link at `0x40080000`); `tools/build.sh --arch
  aarch64` flattens the ELF to `build/kernel-aarch64.bin` with
  `llvm-objcopy`. R9a links no C driver layer (block stubs return -1).
- `user/build.rs`: applies `user/link.ld` (static ELF linked at
  `0x400000`, shared by both arches).

## 6. Disk fixtures (test images)

`tools/mkdisk.py` hand-builds the GPT + FAT32 test disk; `run.sh` invokes it.

```sh
python3 tools/mkdisk.py build/test.img                 # default FAT32 ESP
python3 tools/mkdisk.py --two-fs build/test.img        # + ext4 root + XFS probe stub
python3 tools/mkdisk.py --broken build/test.img        # missing EFI/BOOT/BOOTX64.EFI
python3 tools/mkdisk.py --broken-shim build/test.img   # missing fallback AND shim
python3 tools/mkdisk.py --keys build/test.img          # Secure Boot certs + autorun
python3 tools/mkdisk.py --shell-repair build/test.img  # autorun runs grub-fix repair + YES
python3 tools/mkdisk.py --liar build/test.img          # crafted FAT size lie
python3 tools/mkdisk.py --bigcluster build/test.img    # 4 KiB clusters
python3 tools/mkdisk.py --grub-regen build/test.img    # autorun runs grub-fix install
```

## 7. Troubleshooting builds

| Symptom | Cause / fix |
|---|---|
| `can't find crate for 'core'` | wrong toolchain: use the exact `--target` and `$HOME/.cargo/bin` |
| `kernel/user_program.bin is missing` | build `fantuan-user` first (or use `tools/build.sh`) |
| `clang: command not found` (riscv) | install clang; `kernel-riscv/build.rs` uses it for the C drivers |
| `llvm-ar: command not found` | install LLVM tooling or add its directory to `PATH` |
| `tools/run.sh: OVMF not found` | install `edk2-ovmf`; `run.sh` searches the usual paths |
| `llvm-objcopy: command not found` (aarch64) | install LLVM tools or use `aarch64-linux-gnu-objcopy`; `tools/build.sh` needs one to flatten the raw image |
| `bison: command not found` | install bison (bash's `parse.y`), or build without a shell: `tools/kconfig.py --symbol BASH=N` |
| `fetch-bash-src: no source available` | the bash tarball is not tracked on main; allow the one-time fetch, or `FANTUAN_BASH_TARBALL=/path/bash-5.3.tar.gz`, or `--from-branch` after `git fetch origin fantuan-apps` |
| `kernel/bash_program.bin is missing` but `CONFIG_BASH=y` | run `tools/fetch-bash-src.sh && tools/build-bash.sh`, or use `tools/build.sh`; `CONFIG_BASH=n` / `FANTUAN_BUILD_BASH=0` skips the shell |
| file-size rule failure | every source file must stay <= 300 lines; split it |

## 8. Versioning

The workspace version is the release version (`0.0.4` since the v0.0.4
release; `0.0.3` was M11, `0.0.2` M10, `0.0.1` the M9 release). Banners print `fantuan v0.0.4` on
every arch (i686 says `(i686)`, riscv says `(riscv64)`; the UEFI loader
prints `fantuan-boot v0.0.4`). The BootInfo ABI carries its own
`BOOT_VERSION` (append-only), the syscall `ABI_VERSION` stays 1, and the
syscall numbers live in `abi/src/lib.rs` (append-only, never renumber).
See [DEVELOPMENT.md](DEVELOPMENT.md) for the release checklist.
