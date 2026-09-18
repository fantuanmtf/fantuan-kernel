# Development Guide

How to work on fantuan-kernel: ground rules, the shared-core architecture,
extension recipes, testing and debugging. New maintainers should read
[HANDOVER.md](HANDOVER.md) first, then [DESIGN.md](DESIGN.md) (the
authoritative design), then this file.

## 1. Ground rules (non-negotiable)

1. **English-only artifacts**: code, comments, docs, commit messages.
2. **Docs before code**: change `docs/DESIGN.md` (and the milestone plan)
   before changing behavior; the doc is the record of intent.
3. **300-line limit**: no source file over 300 lines. Split by
   responsibility, not by accident.
4. **Read-only iron rule**: the boot path and every diagnostic is
   read-only. Disk/NVRAM writes are only reachable through the
   `RepairToken` (issued after an explicit `YES`). Never add a write path
   that bypasses it.
5. **Append-only ABI**: `BootInfo` fields are appended (bump
   `BOOT_VERSION`), syscall numbers in `abi/src/lib.rs` are never
   renumbered, and both sides share the crate.
6. **Spike before code**: measure hardware/firmware facts (register
   offsets, handoff registers) with a throwaway spike and record the
   evidence in the docs before writing the driver.
7. **Evidence discipline**: every commit carries a `Verified:` paragraph
   with real command output (boot lines, smoke PASS, warnings count).
8. **Zero warnings** on both targets is part of "done".
9. **No allocator**: `kernel-core` and both kernels are `no_std` without
   `alloc`. Use fixed buffers and `static mut` scratch; `core::fmt` writes
   go through the `kernel-core::log` sink.
10. **One nightly exception**: the planned i686 crate builds with nightly
    `-Z build-std` and `targets/i686-fantuan-none.json` because stable has
    no 32-bit bare-metal target; every other crate stays on stable.

## 2. Repository map

```
abi/        BootInfo layout, PHYS_OFFSET (cfg per arch), syscall numbers
boot/       UEFI bootloader: GOP, RSDP, memory map, kernel load, ExitBootServices
kernel/     x86_64 kernel: arch/*, crypto/*, smbios/*, font, input glue,
            x86-only diag (cpu/gpu) and shell commands, build.rs C/asm glue
kernel-riscv/ riscv64 kernel: main.rs boot, paging, trap, syscall bridge,
            sbi, timer, task, fdt, uart, cpu, drivers, shell table
kernel-core/ portable half: frame, task, syscall, elf, user, log, input,
            time, mem, drv, arch hooks, runtime (UEFI FFI), vfs/*, diag/*,
            bootrepair/*, shell/*
user/       userland test ELF (cfg-split trampoline: int 0x60 / ecall)
drivers/c/  blk registry + blk_ops; ahci, nvme, i8042 (x86), virtio_mmio (riscv)
tools/      build/run/smoke scripts, mkdisk.py, kbd_test.sh
docs/       DESIGN.md, milestone plan, audit, this documentation set
```

## 3. The hook architecture (how shared code stays portable)

`kernel-core` never contains arch code. Each kernel installs function
pointers at boot (same pattern everywhere: `set_ops` + accessor, defaults
degrade harmlessly):

| Hook set | Installed by | Covers |
|---|---|---|
| `arch::set_irq_ops` | x86 `main`, riscv `main` | interrupt save/restore (frame allocator lock) |
| `arch::set_idle` | both | `hlt` / `wfi` |
| `task::TaskOps` | x86/riscv `task` glue | context switch, kernel-entry stack, VM roots, reaping, ticks |
| `user::UserOps` | both | user address-space create/map/free, ELF `e_machine`, log |
| `log::set_sink` | both | byte sink + LF->CRLF translation |
| `input::set_poll` | both | non-blocking console byte |
| `time::set_ns` | x86 (TSC), riscv default 0 | monotonic nanoseconds |
| `mem::set_phys_to_virt` | both | physical -> accessible virtual |
| `drv::DrvOps` | both | drive handle/name/identity, storage BDF |
| `bootrepair::set_auth_apply` | x86 (crypto) | `.auth` bundle verification |

`TaskOps` includes `set_kernel_stack(top, is_user)` and
`init_kernel_stack(top, body)`; riscv uses `sscratch` for the U-mode trap
swap, x86 uses the TSS `rsp0`.

## 4. Extension recipes

### Add a syscall
1. Append the number to `abi/src/lib.rs` (never renumber).
2. Handle it in `kernel-core/src/syscall.rs::dispatch`; keep the arch
   entry thin (x86 `int 0x60` frame, riscv `ecall` trap branch).
3. If user memory must be read, use the arch write/copy bridge pattern
   (x86 STAC/CLAC, riscv SUM with the `in_user_copy` fault classification).
4. Extend `user/src/main.rs` to exercise it; assert it in a smoke phase.

### Add a shell command
1. Implement `fn(&mut Shell, &mut Log, &[&[u8]])` in
   `kernel-core/src/shell/cmds.rs` (portable) or `kernel/src/shell/cmds.rs`
   (x86-only hardware commands).
2. Register it in the per-arch table: `kernel/src/shell/mod.rs`
   (`COMMANDS`) and/or `kernel-riscv/src/shell.rs`.
3. Keep it read-only; write paths must call `vfs::enable_repair_mode()`
   only after a `YES` confirmation, like `cmd_grubfix`.

### Add a storage driver
1. Implement a `struct blk_ops` in C (`drivers/c/`, include `driver.h`),
   register it with `blk_register(&ops, priv)` once probed.
2. Export `k_log`/`k_alloc_page`-style helpers only from `rust_core.h`;
   hardware access goes through the arch primitives it exposes.
3. Link the file in the kernel's `build.rs` (`cc` for x86 with gcc, clang
   for riscv) and call the probe from the kernel's bring-up.
4. The VFS, probe table, boot repair, `diskhealth` and the shell then work
   unchanged — that is the point of the registry. Return `NULL` SMART ops
   when the transport has none (the shared command prints the honest
   "unsupported" line).

### Add a package to the portable core
1. Move the file to `kernel-core/src/...`, replace `crate::serial` with
   `crate::log`, and re-export from the kernel (`pub use kernel_core::x;`)
   so no call site changes.
2. Introduce a hook (section 3) for every remaining kernel/arch
   dependency.
3. Build both targets, boot x86, run the relevant smoke phase, keep every
   file <= 300 lines.

## 5. Testing

| Check | Command | Notes |
|---|---|---|
| both builds, zero warnings | `tools/build.sh` + `tools/build.sh --arch riscv64` | fastest signal |
| full x86 acceptance | `tools/smoke.sh` | 13 phases, ~25 min, needs SMM OVMF for phases 11–13 |
| riscv acceptance | `tools/smoke-riscv.sh` | 3 phases (read-only, repair YES, repair NO) |
| quick boot sanity | `timeout 80 tools/run.sh < /dev/null \| grep ...` | use while iterating |
| targeted shell phase | `timeout 160 tools/run.sh --keys --broken` | autorun exercises 9 commands |

There is no `cargo test` suite: both targets are bare-metal binaries. The
smoke suites are the acceptance tests, and each phase asserts exact serial
lines; when you change an output string, update the corresponding grep.

## 6. Debugging

- **x86**: serial is the source of truth; `--graphics` mirrors it to GOP.
  `tools/kbd_test.sh` drives the QEMU monitor for keyboard tests.
- **riscv**: `-d int -D build/qemu-int.log` records `cause/epc/tval` for
  every trap. `-d int,exec` shows the exact translation-block sequence
  before a hang (grep the last interrupt line).
- **Page faults**: the kernel prints `exc/trap ... scause= stval= sepc=`;
  for a riscv user fault the task is killed and reaped by design.
- **Absolute-pointer rule (riscv)**: the kernel is linked at `0x80200000`
  and executed through the `PHYS_OFFSET` alias, so function pointers and
  jump tables hold link addresses. Kernel code always runs on the kernel
  root; per-task roots are entered only for U-mode (user entry asm, trap
  exit) and around the user-copy bridge. Breaking this causes identity
  jumps that fault under a user root.
- **virtio**: modern transports need
  `-global virtio-mmio.force-legacy=false`; ring fields must be accessed
  as `volatile` (the compiler otherwise caches `used.idx` in poll loops).

## 7. Commits and releases

- Commit style: `Mx.y: short summary`, then a body explaining the what and
  why, ending with a `Verified:` paragraph (commands and observed output).
  Follow the existing history (`git log`) for tone.
- Never commit generated artifacts (`kernel*/user_program.bin` are
  gitignored) or secrets.
- **Do not commit/push/tag unless asked.** The v0.0.1 tag is local.
- Release flow: see `docs/M9_KERNEL_v0.0.1.md` §11 and
  [OPERATIONS.md](OPERATIONS.md) §8.
