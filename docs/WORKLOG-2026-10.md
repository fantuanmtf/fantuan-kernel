# Work log — baseline, refactor, documentation (2026-10)

This file is the working ledger for one batch of work. It exists for two
reasons: so the owner can see progress without reading the diff, and so a
later session (or a later reader) can reconstruct what was done and why
without guessing.

It is **batch-scoped, not permanent**. The durable conclusions fold into
`docs/HANDOVER.md` and `docs/PROGRESS.md` when the batch closes, and this
file is then either deleted or kept as a dated record — it must not become a
second source of truth for project status. Where a number appears here, the
file that owns that number is named alongside it.

Companion plan: the batch plan lives outside the repository (in the owner's
session directory) and is not tracked; §1 and §2 are its tracked summary.

---

## §1 Scope and decisions

### What this batch delivers

1. **A real baseline.** `docs/fantuan-kernel-cvs/README_如何使用.txt` states
   that the CSV planning set was written from public documentation without
   cloning, building or running the project, and that `docs/PROGRESS.md` is a
   *documented claim, not a re-verified result*. Its route table therefore
   opens with **R-001**: establish the true baseline. This batch runs it.
2. **A targeted refactor.** The 300-line ratchet (`tools/check-lines.sh`) is a
   hard gate inside `make check`, and it currently blocks the declared next
   action: `kernel-core/src/graphics/kms.rs` sits at 298/300 and
   `kernel-core/src/vfs/mod.rs` at 300/300, so M13-5 cannot add a line until
   they are split. The split is a prerequisite, not cleanup.
3. **Documentation that matches the code,** and a forward route derived from
   the measured state rather than from the previous claims.

### Decisions taken by the owner before work started

| Decision | Choice |
|---|---|
| Deliverable | Repository documentation, plus the refactor and the full baseline run |
| Refactor force | Includes arch de-duplication (fdt/uart/trap/arch glue reviewed pair by pair) |
| Baseline | Full run — the route is to rest on measurements, not on doc claims |
| As-built architecture write-up | Into `docs/HANDOVER.md` (no new `ARCHITECTURE.md`) |
| Roadmap re-derivation | Into `docs/HANDOVER.md` + `docs/PROGRESS.md`; **the CSV tables are an input and are not modified** |
| aarch64 emulator | Owner installs `qemu-system-aarch64`; work does not wait on it |
| Git | Commits are local; the push command is handed over, never run here |

### Explicitly out of scope

- No rewrite: `docs/fantuan-kernel-cvs/04_决策记录.csv` D-001 says continue the
  existing implementation, so every change here is a move or a correction.
- No new features. M13-5/6/7 and M14-x are *prepared for*, not implemented.
- No shutdown/reboot path is implemented. `CONFIG_SECURE_WIPE` and the README's
  RAM-wipe claim are corrected in documentation and registered as the named
  prerequisite `PWR-1` on the route; implementing power management inside a
  documentation batch is exactly the un-evidenced claim the CSV warns about.
- The CSV tables under `docs/fantuan-kernel-cvs/` are not edited.

---

## §2 Todo

Legend: `[ ]` open · `[x]` done · `[!]` blocked · `[-]` skipped (with reason).

### Phase 0 — baseline (R-001)

- [ ] P0.1 `make doctor`, environment snapshot, back up `.config` (rescue profile)
- [ ] P0.2 `make config-check`, `make check`, `make check-i686`
- [ ] P0.3 `make smoke` (config, gpl, apps, posix)
- [ ] P0.4 Orphan suites: bash, dash, net, net-tls, imager, imager-bad, ntfs, gpu, graphics, input, kms
- [ ] P0.5 `make test-matrix` (smoke.sh 13 phases, bios, riscv, aarch64, iso)
- [ ] P0.6 Classify pass / fail / env-blocked, write `build/baseline/RESULTS.txt`, restore `.config`

### Phase 1 — refactor

- [ ] P1.0 `tools/kconfig_emit.rs`: add the missing `ntfs` entry to `DEFAULTS` and the check-cfg list
- [ ] P1.1 Split `kernel-core/src/graphics/kms.rs` → `kms/{mod,fbobj,crtc}.rs` (M13-5 prerequisite)
- [ ] P1.2 Split `kernel/src/main.rs` → `main.rs` + `boot.rs` + `wiring.rs`
- [ ] P1.3a Split `kernel-core/src/process.rs` → `process/{mod,identity,group}.rs`
- [ ] P1.3b Split `kernel-core/src/process.rs` → `process/{sigstate,proc,vma}.rs`
- [ ] P1.4 Split `kernel-core/src/vfs/mod.rs` → `{mod,mount,name83,repair}.rs` (M13-6 prerequisite)
- [ ] P1.5 `vfs/fd.rs`: update the budget note only (split deferred to M13-6)
- [ ] P1.6a Merge the two FDT parsers into `kernel-core/src/fdt.rs`
- [ ] P1.6b Merge the UART formatting layer into `kernel-core/src/uconsole.rs`
- [ ] P1.7 Ratchet close-out: `tools/line-budget.txt` 10 → 8 lines; fix the `§5` → `§8` citation

### Phase 2 — documentation reconciliation

- [ ] P2.1…P2.19 The nineteen verified discrepancies (see §5 for the list and the twosettled principles)

### Phase 3 — route re-derivation

- [ ] P3.1 as-built architecture section in `docs/HANDOVER.md`
- [ ] P3.2 Capability × architecture matrix (`docs/USAGE.md`)
- [ ] P3.3 New milestones: `ABI-4`, `PWR-1`, `WIPE-1`, `MEM-1`, `POSIX-2`, `NET-USER`, `LIVE-1`
- [ ] P3.4 `docs/PROGRESS.md` "Next action" rewrite
- [ ] P3.5 Optional: `ALL_SMOKE_SET` + `make test-all` so the full matrix is one command

---

## §3 Baseline results

Filled in by Phase 0. One row per suite: result, duration, verbatim reason on
skip, reproduction command, and whether it was already failing before any
change in this batch (`pre-existing at <sha>`).

*Pending.*

---

## §4 Refactor evidence

Filled in as each split lands: file, lines before → after, modules created,
which suites were re-run, and the result. Because this project has no unit
tests (`cargo test` does not exist here; the boot-time self-tests plus the
serial-log assertions in the smoke scripts are the whole verification story),
a "pure move" can only be evidenced by the same smoke assertion holding
before and after.

*Pending.*

---

## §5 Documentation discrepancies and the two principles

Nineteen verified places where documentation contradicts the source. Two
principles govern every edit:

**Principle 1 — a dated snapshot is history and is not edited.** The rule
targets *forward-looking* statements ("current state", "next action", "the
rule is"); rewriting a past snapshot to today's numbers would falsify the
record.

**Principle 2 — do not assert a count; point at the file that owns it.** This
is the only way the recurring class stops rotting. The house style is:

> The 300-line rule is enforced by `tools/check-lines.sh` as part of
> `make check`; the files above the line, each with the reason it is unsplit,
> are in `tools/line-budget.txt` — **that list is the count, and this file no
> longer restates it.**

The nineteen items, worst first, are listed in §5.1 of the batch plan; the
high-severity end is:

1. `README.md` claims the Live profile wipes RAM on clean shutdown
   (`CONFIG_SECURE_WIPE`). The symbol gates no code, no profile enables it
   beyond `default n`, and **the kernel has no shutdown, reboot or power path
   at all** — so "clean shutdown" cannot occur. Documentation-side fix plus
   the named prerequisite `PWR-1`.
2. `docs/ROADMAP_v0.0.2+.md` lists `GRAPHICS_API.md` and `REPAIR_IPC.md` as
   "frozen in M13"; neither file exists and M13-7 is still open.
3. `docs/HANDOVER.md` asserts "no source file > 300 lines", which was already
   false when written (`process.rs` was 911).
4. `docs/HANDOVER.md` says seven files are over the limit and that "the rule
   has no gate yet"; the real figures are ten files and a gate that runs in CI.

Full item list: §2 of the batch plan.

---

## §6 Route after this batch

Filled in by Phase 3. The four findings the CSV set does not record, each of
which adds a prerequisite to its route:

1. **No socket syscall family exists.** The ABI call table stops at
   `SYS_FCNTL = 47`; networking exists only as in-kernel shell commands. The
   CSV's I-010 / R-007 ("migrate the network tools to userspace") therefore
   needs `ABI-4` first, which it does not list.
2. **No shutdown, reboot or power path exists.** `PWR-1` precedes the CSV's
   I-014 / R-010 ("Live persistence, clean shutdown"), and precedes `WIPE-1`.
3. **No COW, futex, `/proc` or file-backed mmap.** The desktop prerequisite
   list is longer than the CSV records.
4. **aarch64 has neither userspace nor storage,** and CI runs no QEMU at all,
   so roughly twenty QEMU suites are manual and rot between runs.

*Pending.*

---

## §7 Open items and hand-over notes

Filled in at close: what needs an owner action, what was deliberately
deferred, and where the next reader should start.

*Pending.*
