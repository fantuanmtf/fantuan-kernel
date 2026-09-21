# Build Guide

How to build fantuan-kernel v0.0.3 (x86_64, i686, riscv64 and aarch64) from source.
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
| `clang` + `llvm-ar` | build the C driver layer for riscv64 | any recent LLVM |
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
```

All crates are version `0.0.3` (`panic = "abort"`, release `opt-level = "z"`).

## 3. Build with the scripts (recommended)

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

## 3.1 Configuration profiles (Kconfig-lite)

The kernel is configured before it is built; a missing `.config` resolves to
the default **minimal** profile (kernel + boot + shell, plus the declared
`BASH` symbol; bash itself arrives with M14). Everything else is opt-in:

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
`tools/kconfig_emit.rs`; the profile, enabled list and config hash appear in
the boot banner. Apps add `CONFIG_APP_*` symbols via
`tools/appctl/appctl.py menu` (see [APPS.md](APPS.md)); bash is vendored but
not built, with the early-port record in `apps/bash/port/`.

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

# P2 userland/POSIX chain (optional; the minimal kernel embeds what exists).
# Order matters: libc archive first, then dash (it links the archive).
tools/build-libc.sh    # libc-fantuan.a + hello/proc-test/ls/cat ELFs
tools/build-dash.sh    # vendored dash 0.5.12 -> kernel/dash_program.bin
```

A clean build must produce **zero warnings** on all targets; treat a new
warning as a build break.

## 5. What the build scripts generate

- `kernel/build.rs`: `asm_defs.inc` from `kernel/src/consts.rs`, the
  256-entry ISR stub table, the embedded user ELF, and compiles the x86 C
  driver layer + assembly stubs with `cc` (`-mcmodel=large`,
  `-mno-red-zone`).
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
| file-size rule failure | every source file must stay <= 300 lines; split it |

## 8. Versioning

The workspace version is the release version (`0.0.3` since the v0.0.3
release; `0.0.2` was M10, `0.0.1` the M9 release). Banners print `fantuan v0.0.3` on
every arch (i686 says `(i686)`, riscv says `(riscv64)`; the UEFI loader
prints `fantuan-boot v0.0.3`). The BootInfo ABI carries its own
`BOOT_VERSION` (append-only), the syscall `ABI_VERSION` stays 1, and the
syscall numbers live in `abi/src/lib.rs` (append-only, never renumber).
See [DEVELOPMENT.md](DEVELOPMENT.md) for the release checklist.
