# M9 → kernel v0.0.1 — Development Plan and Checklist

> Status: living document. Update it before the code of each milestone lands
> (DESIGN.md governing principle 5). This file is the working checklist for
> the RISC-V bring-up (M9.0–M9.5) and the v0.0.1 release; DESIGN.md §14 holds
> the architectural roadmap this plan implements.
>
> Progress: M9.0 (arch consolidation) and M9.1/M9.2 (riscv boot, Sv39, FDT,
> frame allocator, traps, SBI timer, scheduler + kernel-core extraction) are
> DONE and committed; M9.3–M9.5 and the v0.0.1 release remain.

## 1. Purpose and scope

Deliver **kernel v0.0.1**: the x86_64 rescue system keeps every capability it
has today (verified by the full smoke suite), and a second kernel binary
brings the same operating discipline up on RISC-V (QEMU `virt` + OpenSBI)
through M9.5:

1. M9.1 — boot to serial: `kernel-riscv` binary, Sv39, FDT-lite, frame
   allocator, banner.
2. M9.2 — traps and tasks: trap frame, SBI timer, context switch, scheduler,
   reaping; shared code extracted to `kernel-core`.
3. M9.3 — user mode and diagnostics: U-mode tasks, per-task Sv39 tables, ELF
   loading, FDT-based CPU/RAM diagnostics.
4. M9.4 — storage: virtio-mmio block driver behind `blk_ops`; the VFS,
   probe table, boot repair and shell run on RISC-V.
5. M9.5 — audit (technical debt + vulnerability review) and the full test
   matrix.

Non-goals for v0.0.1: SMP, networking, USB, graphics, real-hardware RISC-V
boards, authenticated Secure Boot on RISC-V, chroot/linuxulator (M10).

## 2. Content standards (binding)

These constraints are the acceptance criteria for every checklist item.

1. **English-only artifacts** (code, comments, kernel messages, commits,
   docs); kernel output stays ASCII.
2. **Docs first**: this checklist is updated before the code of each
   milestone slice; DESIGN.md gains the milestone entry when the slice lands.
3. **File size**: target ≤ 300 lines per source file; module-map header and
   `// --- … ---` section banners; split early.
4. **Append-only ABIs**: BootInfo/syscall/ops tables only gain fields;
   architecture differences are compile-time (`cfg(target_arch)`) or
   capability bits, never reordered fields.
5. **Read-only-first**: neither architecture may write to a disk during a
   normal boot; repairs stay behind repair mode + explicit consent.
6. **Evidence discipline**: every item lists deliverable / verification /
   evidence. An item is done only when the evidence (a log line, a command
   output, a hash) is reproduced in the commit's `Verified:` paragraph.
7. **Spike before code** for every new platform fact (register offsets,
   handoff conventions, tool behaviour). No value from memory; cite a
   measured log or a named source.
8. **Commit discipline**: one slice per commit, clear subject, body explains
   the what/why, `Verified:` states the gates actually run.

Anti-hallucination guardrails (mandatory practice, not checklist items):
read before edit; build after every change; never claim a test that was not
run; verify tool/environment availability first; re-run `wc -l` before
committing; keep the verification levels (structure / self-consistency /
trust) explicit in any security claim.

## 3. Versioning and release criteria

- Working version during M9: banners keep the milestone tag (e.g.
  `fantuan (riscv64) M9.1`); crate versions remain 0.1.0.
- **v0.0.1 release criteria** (all must hold):
  1. Every checklist item below is closed with evidence.
  2. x86 full smoke suite green (all phases, including SMM NVRAM).
  3. RISC-V smoke suite green (all phases defined by then).
  4. Zero compiler warnings on both targets.
  5. M9.5 audit findings fixed or explicitly recorded as accepted debt.
  6. All source files ≤ 300 lines.
  7. README, DESIGN and this document consistent with the code.
- Release actions: workspace versions to 0.0.1, banners print
  `fantuan v0.0.1` on both architectures, `git tag v0.0.1`.

## 4. Checklist conventions

Each item has an id (`M9.x-n`), a deliverable, a verification command or
check, and the expected evidence. Items are ordered by dependency; a later
item may not start before its predecessors' evidence exists.

## 5. M9.1 — Boot to serial (`kernel-riscv`)

Architecture decision (approved): a **separate `kernel-riscv` crate** that
depends only on `fantuan-abi` at first. Generic modules move into a shared
`kernel-core` crate **incrementally from M9.2**, each extraction landing
together with the x86 side switching to it and the x86 smoke suite staying
green. No big-bang refactor.

| id | deliverable | verification | evidence |
|---|---|---|---|
| M9.1-1 | `kernel-riscv/` crate: bin, `no_std`, `panic=abort`, `link.ld` at `0x80200000`, `_start` in `global_asm` (stack, BSS zero, a0/a1 preserved) | `cargo build -p kernel-riscv --target riscv64gc-unknown-none-elf --release` | build log, zero warnings |
| M9.1-2 | MMIO NS16550 at `0x10000000` (8N1, byte write + poll) with `puts`/hex helpers | run QEMU, read the UART | banner text in the log |
| M9.1-3 | Boot banner + handoff report: hartid, DTB physical address, milestone tag | QEMU run | banner + `hartid=`/`dtb=` lines |
| M9.1-4 | Tools: `tools/build.sh --arch riscv64`, `tools/run.sh --arch riscv64` (`-machine virt -bios default -kernel … -nographic`), distinct `tools/smoke-riscv.sh` | run each script | scripts exit 0, smoke prints PASS |
| M9.1-5 | Record the measured a0/a1 handoff convention in DESIGN §14 (spike evidence) | doc review | DESIGN §14.1 updated |
| M9.1-6 | x86 regression: full smoke after the workspace change | `tools/smoke.sh` | 13/13 PASS |

Out of scope for M9.1: page tables, allocator, traps beyond a minimal park
handler, any device other than UART.

## 6. M9.2 — Traps, timer, tasks, `kernel-core`

| id | deliverable | verification | evidence |
|---|---|---|---|
| M9.2-1 | BootInfo append-only fields `arch`, `hartid`, `dtb`; `BOOT_VERSION` handling; `PHYS_OFFSET` becomes `cfg(target_arch)` (riscv: `0xFFFFFFC0_00000000`, Sv39-canonical) | x86 full smoke + riscv boot | x86 13/13, riscv banner |
| M9.2-2 | `kernel-core` crate created; `mm/frame` + `mm/lock` extracted; x86 kernel re-exports (no call-site churn) | x86 full smoke | smoke PASS, no diff in behaviour |
| M9.2-3 | Sv39 page tables: 2 MiB megapages for RAM/MMIO, alias at the new PHYS_OFFSET, `satp` switch; `phys_to_virt` shared | riscv memory self-test + map summary | `mm: frame self-test ok`, mapped-MiB line |
| M9.2-4 | Trap entry (`stvec`, sscratch kernel stack, full register frame), exception reporting, minimal TLB/satp helpers | deliberate `ebreak`/bad access | trap dump on serial, no hang |
| M9.2-5 | SBI TIME timer at 100 Hz; tick counter + scheduler hook | tick log at 10 s intervals | `tick: 10 s (switches N)` |
| M9.2-6 | `task` scheduler extracted into `kernel-core` with arch hooks (x86: TSS/iretq; riscv: trap-return entry); context switch (`ra/sp/s0-s11`) | x86 full smoke + riscv demo tasks | x86 PASS; riscv `task N (tid N): hello` |
| M9.2-7 | Task reaping on riscv: Sv39 page-table walk + kernel stack free | riscv user tasks later; kernel-task stress first | `sched: reaped tid N` |
| M9.2-8 | ABI: syscall numbers documented per arch (x86 `int 0x60`, riscv `ecall` planned for M9.3) | DESIGN update | doc |

Out of scope: user mode, storage.

## 7. M9.3 — User mode and diagnostics

| id | deliverable | verification | evidence |
|---|---|---|---|
| M9.3-1 | `elf` loader extracted to `kernel-core`; PHYS_OFFSET-relative mapping works on both arches | x86 full smoke + riscv load | x86 PASS; riscv `elf: loaded …` |
| M9.3-2 | `user` crate cfg-split: x86 `int 0x60` trampoline, riscv `ecall` trampoline (a7 = number, a0..a5 args) | both ELF images run | `userland: hello from tid N` on both |
| M9.3-3 | U-mode entry on riscv (sstatus SPP/SPIE, sepc, satp per task), per-task Sv39 tables, W^X from `p_flags` | riscv user program exits cleanly | `userland: tid N exiting` + reap |
| M9.3-4 | User fault handling: classify from `scause`, kill the task, reap | deliberate fault test | `exc … [user] killing user task N` |
| M9.3-5 | FDT CPU/RAM diagnostics (model, hart count, ISA string), no SMBIOS; RAM pattern test reuses the generic check | riscv `hwdiag`-equivalent output | `cpu: … riscv-virtio` + RAM result |
| M9.3-6 | (Optional) PCIe ECAM catalog at `0x30000000` | device list on virt | catalog lines |
| M9.3-7 | x86 regression: full smoke | `tools/smoke.sh` | 13/13 PASS |

## 8. M9.4 — Storage and the rescue stack on RISC-V

| id | deliverable | verification | evidence |
|---|---|---|---|
| M9.4-1 | `vfs` (part/probe/fat/fat_write/ext4) extracted to `kernel-core`; x86 re-exports | x86 full smoke | 13/13 PASS |
| M9.4-2 | `diag` + `shell` extracted to `kernel-core` (input source arch-selected: x86 serial+PS/2, riscv serial) | x86 full smoke | shell phases PASS |
| M9.4-3 | virtio-mmio block driver (C): scan `0x10001000` slots, device id 2, modern transport, one queue pair, polling; register `blk_ops` | riscv boot with `-device virtio-blk-device` | `blk: virtio registered` |
| M9.4-4 | RISC-V smoke with the test disk: FAT32 reads, ext4 mount, probe table, boot-repair diagnosis | `tools/smoke-riscv.sh` assertions | `vfs: HELLO.TXT`, `ext4: mounted ro`, probe lines |
| M9.4-5 | `diskhealth` degrades honestly on virtio (no ATA SMART): explicit "SMART unsupported for this transport" | riscv smoke | that line, no fake values |
| M9.4-6 | `run.sh --arch riscv64 --disk` attaches the mkdisk image | run | disk visible to the kernel |

Out of scope: writes on RISC-V beyond the existing consent-gated FAT path
(any write path must keep the repair-mode gate; verify with the same
negative smoke greps).

## 9. M9.5 — Audit (technical debt + vulnerabilities)

Technical debt sweep:

| id | check | evidence |
|---|---|---|
| M9.5-1 | `kernel-core` extraction is clean: no duplicated implementations of frame/task/vfs; x86 and riscv only differ in arch modules | file map in the audit report |
| M9.5-2 | serial/console duplication: x86 port I/O vs riscv MMIO behind one API surface | interface listing |
| M9.5-3 | cfg sprawl review: every `#[cfg]` justified; no dead branches | grep count + comments |
| M9.5-4 | scripts: build/run/smoke cover both arches without duplicated logic where avoidable | script review |
| M9.5-5 | dead code / `#[allow(dead_code)]` reviewed and removed where possible | zero warnings |
| M9.5-6 | docs ↔ code consistency (DESIGN, README, this file) | section-by-section check |
| M9.5-7 | file-size rule holds on both kernels and shared crates | `wc -l` output |
| M9.5-8 | RISC-V smoke coverage gap list recorded | report section |

Vulnerability review:

| id | check | evidence |
|---|---|---|
| M9.5-9 | riscv PTE permissions: U/X/W enforced from ELF `p_flags`; kernel pages not U | page-table dump / test fault |
| M9.5-10 | `sstatus.SUM` semantics reviewed (kernel access to user memory must be deliberate; syscall copy path audited) | code + a fault test |
| M9.5-11 | `satp`/ASID handling on task switch; stale TLB risk after teardown | code review + reap stress |
| M9.5-12 | SBI call return values checked (timer/IPI extensions) | code review |
| M9.5-13 | trap stack: sscratch switching cannot recurse into a user stack; trap frame size bounded | code review + deliberate nested fault |
| M9.5-14 | ecall argument validation (user pointers validated by page tables; no kernel address accepted) | code + fault test |
| M9.5-15 | reaping correctness on Sv39: no double free (frame ownership bitmap), user half only, kernel tables shared | code + stress |
| M9.5-16 | x86 hardening intact after refactors (NX/SMEP/SMAP, STAC/CLAC, RepairToken) | x86 smoke assertions |
| M9.5-17 | read-only iron rule on both arches (no boot-path writes) | negative smoke greps |
| M9.5-18 | crypto claims stay scoped (structure/self-consistency, no chain-trust claim) | docs wording |

## 10. Test matrix

| suite | scope | gate |
|---|---|---|
| `tools/smoke.sh` | x86: 13 phases (boot, repair, ext4, fixtures, shell, keyboard/GOP, NVMe, Secure Boot, SMM NVRAM) | green before and after every slice |
| `tools/smoke-riscv.sh` | riscv: boot banner, memory, traps, tasks, userland, VFS/probe, boot-repair diagnosis (grows with the milestones) | green from M9.1c onwards |
| `crypto-selftest` | SHA-256/RSA KATs, both arches | PASS |
| fixture hashes | mkdisk variants byte-identical after tooling refactors | sha256 comparison |
| build | both targets, zero warnings | `build.sh --arch x86_64` and `--arch riscv64` |

## 11. Release checklist (kernel v0.0.1)

1. Close every item in §5–§9; record the audit report in the release commit.
2. Bump workspace crates to 0.0.1; banners print `fantuan v0.0.1` on both
   architectures.
3. Update README (current state), DESIGN (milestones M9.1–M9.5 done, M10
   planned), and this document (status header).
4. Run the full test matrix one final time on a clean build.
5. Tag `v0.0.1`; keep the tag local until an explicit push is requested.

## 12. Deferred / M10

- **M10 — chroot compatibility (design first)**: linuxulator-style Linux
  ELF + syscall translation and a BSD-ABI path, so the rescue system can
  chroot into the target and run its own tools. Deliverable before code:
  `docs/M10_CHROOT.md` (scope, ABI risks, ELF/syscall surface, staging).
  Not part of v0.0.1.
- Secure Boot authenticated updates on RISC-V, real-board support, SMP,
  networking, USB, graphics: post-v0.0.1.

## 13. Risk register (with mitigations already in force)

| risk | mitigation |
|---|---|
| arch refactor destabilises x86 | one slice per commit, full x86 smoke as the gate |
| RISC-V platform assumptions wrong | spike-before-code; a0/a1 and UART measured first |
| ABI drift breaks the x86 boot chain | append-only fields; x86 smoke after each ABI change |
| shared-code extraction produces subtle regressions | extract with re-exports, no call-site churn, smoke gate |
| cfg sprawl hides dead paths | M9.5-3 review; compiler warnings are errors of process |
| virtio-mmio complexity (queues/features) | polling, modern transport, one queue, spike the registers first |
| tests silently stop covering something | M9.5 coverage-gap list; negative greps for the read-only rule |
