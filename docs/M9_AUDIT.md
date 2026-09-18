# M9.5 — Audit Report (technical debt + vulnerabilities)

> Scope: the RISC-V port (M9.0–M9.4) and the `kernel-core` extraction.
> Method: static review with file:line evidence, fresh builds for both
> targets, the full x86 smoke suite and the riscv smoke. Findings marked
> RISK were either fixed in the M9.5 commit or listed as recorded debt.

## 1. Technical debt

| id | verdict | evidence |
|---|---|---|
| M9.5-1 | PASS | Single-sourced in `kernel-core`: frame/task/syscall/elf/user, vfs/*, diag/*, bootrepair/*, shell/*, runtime, log, drv/input/time/mem/arch hooks. `kernel/` keeps arch/*, crypto/*, smbios/*, font, kbd and glued commands; `kernel-riscv/` keeps paging/trap/syscall/sbi/timer/task/fdt/uart/cpu/drivers. No duplicated implementation found. |
| M9.5-2 | RISK (minor, recorded) | One sink API (`kernel-core/src/log.rs`) plus per-arch drivers is in place, but LF→CRLF and hex/dec formatting are still duplicated between `kernel/src/arch/x86_64/serial.rs` and `kernel-riscv/src/uart.rs`. Moving the formatting helpers into `kernel-core::log` is post-v0.0.1 cleanup. |
| M9.5-3 | PASS | 5 non-test `#[cfg]` sites total: `abi/src/lib.rs` (PHYS_OFFSET per arch), `kernel/src/arch/mod.rs` (x86 crate guard), `user/src/main.rs` (int 0x60 / ecall trampolines). No dead branches. |
| M9.5-4 | RISK (minor, recorded) | `tools/build.sh`/`run.sh` now export the cargo PATH; arch branches are separate but `run.sh` still pre-scans its flags twice. A shared `env.sh` remains post-release cleanup. |
| M9.5-5 | PASS | Zero warnings on both targets after removing stale `#[allow(dead_code)]` on `surface_scan`/`ScanResult` and the unused `blk_read` extern in `diag/diskhealth`. Remaining allows are asm-referenced or deferred features (`probe::FsType::Ufs`, `part::Table.kind`). |
| M9.5-6 | PASS | README, DESIGN §14 and this plan updated to M9.4/kernel-core; banner tags updated. Version strings move to v0.0.1 with the release commit. |
| M9.5-7 | PASS | No tracked `.rs/.c/.h` file exceeds 300 lines (max 297). |
| M9.5-8 | closed for repair writes | The riscv smoke now has three phases: read-only boot (with the negative "no writes" grep), repair YES (FIXED.TXT self-test + fallback shim copy over virtio-blk), repair NO (gate aborts, nothing written). Remaining gaps: NVRAM/SMM, Secure Boot fixtures, PS/2 + GOP mirror, SMBIOS, AHCI/NVMe transports, crypto-selftest, `diskhealth --scan`, and the malicious disk fixtures (`--liar`, `--bigcluster`, `--keys`) — no riscv counterpart exists or is needed for the arch-specific ones. |

## 2. Vulnerability review

| id | verdict | evidence |
|---|---|---|
| M9.5-9 | PASS | `paging.rs::map_user_page` sets U only on leaves; W/X come from `Prot`; non-leaf PTEs carry no U (QEMU 10 rejects it); kernel 2 MiB/MMIO mappings are never U; the user root shares only entries 256..512. Deliberate fault: `exc 15 [user] scause=0xf stval=0x500000` in the riscv smoke. Write-to-Rx W^X fault test remains a gap. |
| M9.5-10 | RISK → FIXED | `syscall.rs::clear_user_copy` now also clears `sstatus.SUM`; previously the fault path called `task::exit` before `set_sum(false)`, leaking SUM=1. The timer cannot preempt the copy window (ecall entry clears SIE). |
| M9.5-11 | PASS | `set_root` always writes satp + `sfence.vma` (ASID 0, global flush); `arch_switch` deliberately stays on the kernel root; a freed root's frames only return to the allocator while another root is active. |
| M9.5-12 | PASS | `sbi.rs` checks the SBI error return before consuming a1; `timer.rs` logs nonzero returns and never uses the value. |
| M9.5-13 | PASS | The trap frame is exactly 272 bytes (`[u64; 32] + sepc + sstatus`); entry swaps sscratch, restores the S-mode invariant (sscratch=0), and arms the kernel stack top per user task. A nested bad-pointer fault is classified by `in_user_copy()` and kills the task. |
| M9.5-14 | PASS (hardened) | `syscall::write` caps len at 512 and rejects null/kernel-half pointers (`ptr`, `ptr+len` ≥ `PHYS_OFFSET`; no wrap possible). ecall handling is now gated on `from_user`. Kernel low addresses are unmapped in a user root, so a bad pointer kills the task. |
| M9.5-15 | RISK → FIXED | `free_user_root` walks VPN2<256 only and every frame free is rejected by the allocator's ownership bitmap (no double free). The `spawn_user` error paths now call `free_user_root` instead of leaking the root and its ELF mappings. |
| M9.5-16 | PASS | x86 NX/SMEP/SMAP, STAC/CLAC around the syscall copy, and the `RepairToken` gate survived the extraction; the smoke phase asserts `mmu: nx true smep true smap true`. |
| M9.5-17 | PASS | The only `blk_write` call site is `vfs/fat_write.rs`, reachable only through a `RepairToken`; `Runtime::set_variable` is called only from `bootrepair::repair`, `install::run` (shell YES) and the secure-boot enrollment path. Boot paths call `diagnose` only; negative smoke greps hold. |
| M9.5-18 | PASS | README/DESIGN scope kernel checks to structure and self-consistency; chain trust is explicitly firmware-decided and authenticated Secure Boot is a RISC-V non-goal. |

## 3. Fixes landed with this audit

0. Follow-up hardening (post-release wrap-up): the fallback shim copy used a
   32 KiB stack buffer, which overflows the 16 KiB riscv boot stack (kernel
   task stacks are 16 KiB on both arches) and corrupted memory during the
   first riscv repair run. The window is now 4 KiB, and the riscv smoke
   exercises the whole repair path.
1. `sstatus.SUM` cleared on the user-copy fault path (M9.5-10).
2. `spawn_user` error paths free the user root (M9.5-15).
3. ecall handling gated on U-mode origin (M9.5-14).
4. Stale `#[allow(dead_code)]` and an unused extern removed (M9.5-5).
5. `tools/build.sh`/`run.sh` export the cargo PATH (M9.5-4).
6. README state/quickstart, DESIGN §14 status and banner tags updated (M9.5-6).
