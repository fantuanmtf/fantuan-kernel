# Build Guide

How to build fantuan-kernel v0.0.1 (x86_64 and riscv64) from source.
For *running* what you built see [USAGE.md](USAGE.md); for the CI-style
checks see [OPERATIONS.md](OPERATIONS.md).

## 1. Toolchain requirements

| Tool | Purpose | Notes |
|---|---|---|
| Rust stable + `rustup` | kernels, bootloader, userland | targets below |
| `x86_64-unknown-none` | x86_64 kernel, userland | add target |
| `x86_64-unknown-uefi` | UEFI bootloader | add target |
| `riscv64gc-unknown-none-elf` | RISC-V kernel, userland | add target |
| `qemu-system-x86_64` + OVMF (`edk2-ovmf`) | run/test x86_64 | SMM OVMF optional |
| `qemu-system-riscv64` (>= 9) | run/test riscv64 | OpenSBI `fw_dynamic` ships with QEMU |
| `clang` + `llvm-ar` | build the C driver layer for riscv64 | any recent LLVM |
| `python3` | disk fixtures (`tools/mkdisk.py`) | 3.8+ |
| `objcopy` (binutils) | x86 kernel ELF -> flat binary | host binutils is fine |

Install the Rust targets once:

```sh
rustup target add x86_64-unknown-uefi x86_64-unknown-none riscv64gc-unknown-none-elf
```

The system `cargo` may lack bare-metal targets; the scripts export
`$HOME/.cargo/bin` themselves, so use `tools/*.sh` or export it in your
shell (`export PATH="$HOME/.cargo/bin:$PATH"`).

## 2. Workspace layout

```
abi/           fantuan-abi    BootInfo ABI + syscall numbers + PHYS_OFFSET
boot/          fantuan-boot   UEFI bootloader (x86_64-unknown-uefi)
kernel/        fantuan-kernel x86_64 kernel (x86_64-unknown-none)
kernel-core/   kernel-core    portable half shared by both kernels
kernel-riscv/  kernel-riscv   riscv64 kernel (riscv64gc-unknown-none-elf)
user/          fantuan-user   the userland test program (both arches)
drivers/c/     C driver layer: blk.c + blk_ops + ahci/nvme/i8042/virtio_mmio
tools/         build/run/smoke scripts + mkdisk.py
docs/          DESIGN.md (authoritative), milestone/award docs, this set
```

All crates are version `0.0.1` (`panic = "abort"`, release `opt-level = "z"`).

## 3. Build with the scripts (recommended)

```sh
tools/build.sh                 # x86: userland -> kernel -> bootloader
tools/build.sh --arch riscv64  # riscv: userland -> kernel-riscv
```

Artifacts:

| Artifact | Path |
|---|---|
| x86 userland (embedded into the kernel by `kernel/build.rs`) | `kernel/user_program.bin` |
| x86 kernel ELF / flat binary (copied to the ESP by `run.sh`) | `target/x86_64-unknown-none/release/fantuan-kernel` |
| UEFI bootloader | `target/x86_64-unknown-uefi/release/fantuan-boot.efi` |
| riscv userland (embedded by `kernel-riscv/build.rs`) | `kernel-riscv/user_program.bin` |
| riscv kernel (loaded by OpenSBI at `0x80200000`) | `target/riscv64gc-unknown-none-elf/release/kernel-riscv` |

Both `user_program.bin` files are generated and git-ignored; never edit them.

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

# library-only checks
cargo build -p kernel-core     --target x86_64-unknown-none --release
cargo build -p kernel-core     --target riscv64gc-unknown-none-elf --release
```

A clean build must produce **zero warnings** on both targets; treat a new
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
| file-size rule failure | every source file must stay <= 300 lines; split it |

## 8. Versioning

The workspace version is the release version (`0.0.1` at the v0.0.1 tag).
Banners print `fantuan v0.0.1` on both arches. The BootInfo ABI carries its
own `BOOT_VERSION` (append-only) and the syscall numbers live in
`abi/src/lib.rs` (append-only, never renumber). See [DEVELOPMENT.md](DEVELOPMENT.md)
for the release checklist.
