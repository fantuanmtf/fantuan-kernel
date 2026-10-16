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

- [x] P0.1 `make doctor`, environment snapshot, back up `.config` (rescue profile)
- [x] P0.2 `make config-check`, `make check`, `make check-i686`
- [x] P0.3 `make smoke` (config, gpl, apps, posix) — 4/4 PASS
- [x] P0.4 Orphan suites: 9 PASS, 1 suspect FAIL (`net`, re-run pending) — 35 min
- [x] P0.5 `make test-matrix`: x86 13/13, bios 2/2, riscv 3/3 (after 22+23), aarch64 3/3 (after 23), iso FAIL (finding 24)
- [x] P0.6 Classify: 25 suites, 23 PASS, 1 FAIL, 1 resolved by fix, 0 env-blocked
- [ ] P0.7 Re-run `smoke-net` with the machine unloaded, to settle whether its failure was the load
- [x] P0.8 Fix the two pre-existing failures that block Phase 1 verification on riscv/aarch64 (findings 22, 23) — commits `e2e166d`, `453f1e9`

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

Baseline tree: `d7f6444` (the worklog commit) plus the working tree as found.
One row per suite: result, verbatim reason on skip, reproduction command, and
whether it was already failing before any change in this batch.

Environment as measured (full dump: `build/baseline/00-env.txt`): rustc
1.98.1, QEMU 10.2.3, clang 22, python 3.14.7, 4 CPUs. All four Rust targets
installed (`x86_64-unknown-none`, `x86_64-unknown-uefi`,
`riscv64gc-unknown-none-elf`, `aarch64-unknown-none`).

### Compile and gate

| Step | Result | Note |
|---|---|---|
| `make doctor` | PASS, with one false warning | See discrepancy 21 below |
| `make config-check` | PASS | `config: OK (6/21 symbols enabled, profile rescue)` |
| `make check` | PASS | 19.6 s warm; ratchet `OK (10 file(s) over 300 lines, all budgeted)`; seven crates, zero warnings |
| `make check-i686` | PASS | 11.1 s; `build/kernel-i686.bin: 182636 bytes` |

### `make smoke` (`SMOKE_SET = config gpl apps posix`)

| Suite | Result | Evidence |
|---|---|---|
| config | **PASS** | 8 assertions: net-profile cargo tree on x86_64 *and* aarch64, net markers, minimal profile free of net/rump/tls/rescue strings, rescue profile, 6 MiB budget against the 300 MiB limit with the 1-byte simulation rejecting, rebuild-set incrementality (fantuan-abi untouched), no-shell profile |
| gpl | **PASS** | 7 assertions, including the `origin/fantuan-apps` mirror byte comparison — the fetch succeeded, so the assertion actually ran rather than skipping |
| apps | **PASS** | 9 assertions: add/sync/verify/menu/sbom/remove plus the GPL firewall both ways |
| posix | **PASS** | 11 assertions: ELF embedded and spawned, argv, pid, malloc/brk, cwd, tmpfs read-back, pipe, uptime, exit code, reap |

### Orphan suites (P0.4)

Ten distinct suites — `smoke-net.sh` invokes `smoke-net-tls.sh` itself at
line 242, so net-tls is covered transitively, not separately.

| Suite | Result | Duration | Evidence |
|---|---|---|---|
| bash | **PASS** | 371 s | `[0/5]` deleted the artifacts and proved `tools/build.sh` alone rebuilds and embeds bash, marker `38ec6a3028d8…` matching both the ELF `strings` and the artifact hash; then `-c`, interactive, arithmetic, variables, functions, pipelines, redirects, `$(...)`, SIGINT (`back=130`), reaps, `sh`=bash and dash still selectable |
| dash | **PASS** | 146 s | `-c`, interactive, pipes, redirects, scripts, SIGINT, reaps |
| net | **FAIL** | 778 s | loopback PASS, slirp PASS, **dns tools FAIL** — but that was under load; the unloaded re-run reached `dns tools` PASS and failed later, at `tls udp`. See finding 26 |
| imager | **PASS** | 202 s | verified round trip, size gate, YES gate, read-only branch linked |
| imager-bad | **PASS** | 182 s | abort, `--continue` zero-fill, report ranges/counts, quick verify |
| ntfs | **PASS** | 194 s | ro mount, listing equality, resident + fragmented hashes, corrupt rejection, **no writes** |
| gpu | **PASS** | 315 s | std + cirrus identity/BAR, 64-bit aperture mapped, pcie n/a, no ACPI TZ, shell command |
| graphics | **PASS** | 209 s | double-buffer demo + damage containment, GOP + VBE |
| input | **PASS** | 58 s | ring self-test, PS/2 mouse → demo cursor, key stop |
| kms | **PASS** | 80 s | dumb buffers + ADDFB/SETCRTC/PAGE_FLIP + events + cleanup |

9 PASS, 1 FAIL, 0 env-blocked.

**The `net` failure is recorded as suspect, not as a verdict,** and
deliberately not yet written into `PROGRESS.md` as a known issue. Reason:
this run overlapped the owner's `emerge` compiling QEMU at `-j4` on a 4-CPU
box (load average 11–16 throughout). The failing phase is "net dns tools",
which boots through `timeout --signal=KILL ${DNS_TOOLS_TIMEOUT:-180}
./tools/run.sh --net` — and `run.sh` without `--no-build` runs
`tools/build.sh` **inside that same 180-second window**. That phase is the
only one in the suite that switches profile (`--profile net`), so it is the
only one that forces a full `kernel-core` + `kernel-net` + `kernel` rebuild
before the VM starts. The captured log confirms it: the transcript ends at
`rump: pool self-test ok` during network bring-up, and contains **zero**
`shell: ready` / `login shell` markers, so the kernel never reached a shell
before the kill. A functional DNS failure would show the shell and a failed
lookup, not a truncated boot under a profile rebuild. P0.7 re-runs it
unloaded; only if it fails again does it become a finding.

**Finding from this: several suites bound a rebuilding `run.sh` with a
timeout sized for a boot.** `smoke-bash.sh:137` (120 s) and
`smoke-net.sh:206` (180 s) both put `tools/build.sh` inside the wall-clock
bound intended for the QEMU run. On an idle machine the rebuild is a stamp
hit and nearly free, which is why this has not bitten before; under load it
is the dominant cost and the run is killed mid-boot. Not fixed here — the
baseline should be measured first — but recorded so the fix (pass
`--no-build`, as `tools/smoke.sh` already does for its 13 phases) is not
rediscovered from scratch.

### Cross-architecture matrix (P0.5)

| Suite | Result | Note |
|---|---|---|
| x86 (`tools/smoke.sh`) | **PASS 13/13** | read-only boot path, broken-ESP repair via YES, ext4 root + fstab + /boot inventory + systemd/UKI, firmware-default SMBIOS, `grub-fix install`, crafted-FAT truncation, 4 KiB-cluster zero-pad, shell autorun + surface scan + crypto KATs + GPU report + login shell, confirmation-gated repair, PS/2 keyboard with serial mirrored to GOP, NVMe + VFS + SMART + boot repair, Secure Boot `.auth`, NVRAM delete/create/keep |
| bios | **PASS 2/2** | `bios x86_64` (MBR → LBA → E820 → BootInfo → ATA → long mode → kernel/tasks/shell) and `bios i686` (BootInfo → paging → allocator → IDT/PIC/PIT → scheduler → ring 3 ELF32 / int 0x80 → PIO ATA VFS) |
| riscv | **PASS 3/3** | after the `sscratch` fix (22) and the feeder fix (23): phase A boot/Sv39/traps/timer/userland/virtio-blk/VFS/shell; phase B repair YES; phase C repair NO |
| aarch64 | **PASS 3/3** | R9a direct-FDT boot (PL011, 4K MMU + direct map, GICv2, 100 Hz, BRK resume, tasks, shell) + R9b virtio-net MMIO + DHCP + IPv4/TCP + HTTP/DNS/wget + TLS/HTTPS/UDP. **First full runtime run of aarch64 on this host** |
| iso | **FAIL** | pre-existing regression, finding 24 — cannot pass on any default build |

**Baseline total: 25 suites run — 23 PASS, 1 FAIL (`iso`), 1 resolved to
PASS after a fix (`riscv`). Zero environment-blocked**; the single gap, the
aarch64 emulator, was closed during the batch.

### Findings 22–25, from the baseline

**22. riscv: a task boundary left `sscratch` stale, and the next S-mode trap
faulted. FIXED** (`e2e166d`). `smoke-riscv` failed 3/3 with an identical
signature — `scause=0xf stval=0x303e58 sepc=0xffffffc0802000a2 [kernel]`, a
store page fault inside `trap_entry`. `trap_entry` swaps `sscratch` and `sp`
(`csrrw t0, sscratch, sp`), so a trap from U-mode leaves `sscratch` holding
the *user* sp while the kernel runs in S-mode. The asm epilogue clears it,
but the epilogue does not run when the handler never returns, and two
handlers never return: the fault branch calls `task::exit(1)`, and the
`ecall` branch calls `syscall::dispatch`, which routes `SYS_EXIT` to
`task::exit` too. `sscratch` kept the dead task's user sp, the next S-mode
trap (the 100 Hz timer is enough) swapped it into `sp`, and the trap frame
was stored at an unmapped user address — the kernel root carries no user
mappings in S-mode. `stval` 0x303e58 sits just under `USER_STACK_TOP`
(0x304000), which identifies it as a user sp. Fixed by clearing `sscratch`
once at the top of `trap_dispatch`, which also covers a nested trap taken
mid-handler.

This is the item the docs have carried as the "intermittent riscv
`uart::log_bytes` fault" since M10, hit at every checkpoint and confirmed
pre-existing on pristine HEAD during P1. It is not intermittent and it is not
the UART: it is a task-boundary bug, and the earlier `scause=0xd` readings
are the same stale value going into a load instead of a store. PROGRESS's
housekeeping line ("hunt the intermittent riscv `uart::log_bytes` fault") is
answered by this.

**23. Two smoke suites fed the serial shell on a fixed sleep. FIXED**
(`453f1e9`). `smoke-riscv.sh` used `(sleep 5; printf ...)` and
`smoke-aarch64.sh` used `(sleep 4; printf ...)`; both assume the boot finished
inside that window, and when it has not the bytes are lost rather than
buffered, so the commands never arrive and the suite fails on markers the
kernel would have printed. This is a host-speed dependency presenting as a
kernel failure, and it cost a misread here: aarch64's R9a failed its output
assertions while its boot log was complete and healthy, and feeding the same
machine at 8 s worked. Both now poll the run log for `shell: ready` before
writing — the pattern `tools/smoke-bash.sh` already used. Assertions
unchanged.

**24. `tools/smoke-iso.sh` cannot pass on a default build — the embedded bash
broke the CD-preload budget. NOT FIXED** (a design change, not a batch item).
`tools/build-bios.sh:59` fails when the BIOS kernel exceeds `0x70000`
(448 KiB), because the El Torito variant is preloaded at 0x7C00 and carries
the kernel at 0xC000, under the 0x9FC00 top of usable low RAM. The kernel is
**1,246,392 bytes — 2.7× over**. The cause is in the artifacts:
`kernel/bash_program.bin` is 743,312 bytes and is embedded, and `CONFIG_BASH`
defaults to `y` **even in the `minimal` profile** (a schema default, not a
profile entry), so `smoke-iso.sh`'s `--profile minimal` does not avoid it. The
CD path therefore broke when bash was embedded (P3, 2026-10) and nothing
caught it: `smoke-iso` is not in CI, and `make test-matrix`, which includes
`iso`, is run by hand. Two decisions it raises: whether the CD variant should
build with `CONFIG_BASH=n` (743,312 + ~503,080 still exceeds 448 KiB, so that
alone is not enough), and whether the preload strategy should change to
loading the kernel from the ISO filesystem instead. Not attempted here.

**25. `tools/check-toolchain.sh` searches a narrower OVMF path list than
`tools/run.sh`, so `make doctor` warns falsely** — carried below as finding
21, which this restates: it also checks only the two x86_64 Rust targets and
only `qemu-system-x86_64`, which is why the missing `qemu-system-aarch64` was
never reported and had to be found by hand.

**26. `smoke-net`'s load failure was the load — confirmed — and a different
failure sits behind it. NOT FIXED.**

The unloaded re-run settles the original question: `net dns tools` **PASSES**
on an idle machine (`net: dhcp lease 10.0.2.15/24`, `net: dns ok`, `net: ping
test.fantuan ok`, `net: wget ok` with matching hashes). The earlier failure
was the `DNS_TOOLS_TIMEOUT=180` window containing a full `--profile net`
rebuild, exactly as predicted, and it is **not** a known issue to file against
the kernel. The "several suites bound a rebuilding `run.sh` with a
boot-sized timeout" note above stands as the real defect there.

The same re-run then reached the TLS/UDP phase and failed it — everything
network-side passed (`net: https get ok` with the pinned CA, `net: udp host
ok (tx=4 rx=4 bytes=1024)`, `net: ext skip`) and the log ends on:

```
exc 1 (#DB debug) err=0x0 rip=0xffff8000010702a7
```

`addr2line` resolves that rip to **`kernel_core::process::wait4`**, and there
is no deliberate `int1`/`#DB` demo anywhere in the tree — `EXC_DEBUG` is only
a name in the exception table — so this is an unexpected debug exception in
`wait4`. It is not in `PROGRESS.md`'s known issues and not in the task list.
The failing assertion is the built-in shell's `wget https://…`, which the
feeder sends after `exit` drops out of the login shell; the log never shows
`shell: login shell exited`, so the exception lands around that transition.
No x86 source was touched anywhere in this batch, so this is pre-existing.

Not investigated further here — it wants the same treatment the riscv
`sscratch` bug got (reproduce, identify the instruction, fix as its own
commit), and that is a kernel-debugging task rather than part of this batch.
Recorded so the next session starts from the rip rather than from "TLS is
broken", which is what the assertion failure reads as and is not.

### A correction about this agent's own environment, recorded because it
### invalidated measurements

`grep` in this session is a shell **function** exported by the Claude Code
harness that aliases to `ugrep -I`, and `-I` **skips files it considers
binary**. The riscv log carries one NUL byte (a CPU-model string copied out of
the FDT), so `file` calls it `data` and every `grep -q` inside
`tools/smoke-riscv.sh` returned false — the suite could not pass on any
kernel, including a correct one, and several early "failures" in this batch
were that rather than the kernel. All suite runs after the discovery were made
with the function unexported. This is a property of the agent harness, not of
the repository: a normal shell's GNU grep 3.12 matches those lines and returns
0. It is recorded because a reader of this log's earlier drafts would
otherwise inherit a false conclusion. The NUL byte is harmless under GNU grep
but is worth a look on its own account.

### New discrepancy found while running the baseline

**21. `tools/check-toolchain.sh` searches a narrower OVMF path list than
`tools/run.sh`, so it emits a false warning.** The doctor prints
`[warn] OVMF firmware not in standard paths` on this machine, but OVMF **is**
present and usable: `/usr/share/edk2/OvmfX64/` holds `OVMF_CODE.fd` and
`OVMF_VARS.fd`. `tools/run.sh:198` searches five directories including
`/usr/share/edk2/OvmfX64`; `tools/check-toolchain.sh:62` searches three and
omits it (and the `/usr/share/edk2-ovmf` symlink that resolves to the same
place). Separately, the doctor checks only the two x86_64 Rust targets and
only `qemu-system-x86_64`, which is why the missing `qemu-system-aarch64` was
never reported by `make doctor` — it had to be found by hand.

---

## §4 Refactor evidence

Filled in as each split lands: file, lines before → after, modules created,
which suites were re-run, and the result. Because this project has no unit
tests (`cargo test` does not exist here; the boot-time self-tests plus the
serial-log assertions in the smoke scripts are the whole verification story),
a "pure move" can only be evidenced by the same smoke assertion holding
before and after.

| Step | Before → after | Suites re-run | Result |
|---|---|---|---|
| kms (`96f0d9e`) | `kms.rs` 298 → `mod` 58 + `fbobj` 195 + `crtc` 88 | kms, graphics, input | PASS |
| main (`0c62732`) | `main.rs` 418 → `main` 94 + `boot/mod` 241 + `boot/report` 87 + `wiring` 125 | config, posix, bash | PASS |
| process (`0bfd019`) | `process.rs` 911 → `mod` 185 + `lifecycle` 273 + `vma` 201 + `sigstate` 144 + `identity` 102 + `group` 75 | posix, dash, bash | PASS |
| vfs (`39e80f5`) | `vfs/mod.rs` 300 → `mod` 66 + `mount` 135 + `name83` 92 + `repair` 51 | posix, ntfs, imager | PASS on x86; **broke riscv** (see below) |
| fdt (`merge`) | riscv 215 + aarch64 240 → `kernel-core/fdt` 257 + shims 33 + 41 | riscv, aarch64 | see below |
| uart (`merge`) | riscv 105 + aarch64 127 → `kernel-core/uconsole` 110 + drivers 43 + 65 | riscv, aarch64 | see below |

`tools/line-budget.txt` went from ten files to eight: `kernel/src/main.rs` and
`kernel-core/src/process.rs` left the list entirely, as the ratchet requires
when a file drops back under the line. Nothing was added to it.

### What the vfs split actually uncovered

The vfs split's moved code is **byte-identical** to what it replaced — verified
with `diff` against `git show 39e80f5^:kernel-core/src/vfs/mod.rs`, which is
how the search was steered away from the code and towards the environment. It
still broke riscv, and the way it broke was the interesting part: two runs of
the same tree failed differently, once with an instruction page fault at 0
after `cat HELLO.TXT`, once with the kernel re-entering `_start` **21,971
times** in a 60-second run, printing `boot: hartid=0xffffffc080222d41
dtb=0x37` each time. Two symptoms from one tree is memory corruption.

`git bisect` by hand put it on the vfs commit (riscv: PASS at `0bfd019`, FAIL
at `39e80f5`). The cause was not the split at all: **the riscv boot stack was
16 KiB** and `bootrepair::diagnose` runs on it, down a chain that reaches a
4 KiB buffer in `fstab::parse_ext4`. That had been marginal for a long time —
an overflow writes below the stack into neighbouring `.bss`, so what breaks
depends on the linker's layout — and a small change in inlining was enough to
tip it. Widening to 64 KiB fixed it outright on the same tree, which is the
proof. x86_64 has always used 64 KiB here; riscv was the outlier. Fixed in
`a705c5e`.

Two things fall out of that worth carrying forward:

- **aarch64 is deliberately still at 16 KiB.** Raising it too was tried and
  made things worse in an unrelated way: the `.bss` shift surfaced a PL011
  interleaving problem where two writers share the UART (`stask 1 (tid 1)` and
  `hell: ready` in one log). That port does not run `diagnose` — it has no
  storage transport — so it never had the deep chain. Its comment now records
  the cliff and says to raise it before wiring diagnose in.
- **That interleaving is a real, pre-existing bug**, the same class as the x86
  COM1 issue `2cfacbe` fixed: two writers on one UART. It is not caused by any
  change here; a change here only made it visible. Not investigated.

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

The batch is paused here for the owner to review and push. Phases 2 and 3 have
landed in part (see §2); the refactor in Phase 1 has **not** started, and
nothing in it has been attempted — the tree carries no half-finished split.

### What is done and committed

| Commit | What |
|---|---|
| `d7f6444` | this ledger |
| `1705b97` | documentation reconciled against the source (18 forward-looking claims) |
| `5d07b14` | dash's manifest no longer calls it the default shell + `appctl relock` |
| `e2e166d` | **riscv `sscratch` fix** — the task-boundary store fault (finding 22) |
| `453f1e9` | **smoke feeders wait for `shell: ready`** (finding 23) |
| `02a16a5` | the measured baseline and its findings |

### Decisions taken, recorded so they are not re-litigated

- **The CD/ISO preload strategy is to be redesigned**, not patched. The owner
  chose this over disabling bash in the CD variant or raising the budget. The
  direction is to stop preloading the kernel in the El Torito image's low-RAM
  window and load it from the ISO filesystem instead, which removes the
  448 KiB constraint entirely rather than negotiating with it. This touches
  `boot-bios/stage2*` and `tools/{build-bios,mkiso}.sh` and is a milestone of
  its own; it is **not** part of this batch. Until it lands, `make test-matrix`
  cannot pass, because it includes `iso`.
- **The `wait4` `#DB` is recorded, not chased** (finding 26). The owner chose
  to defer it. The next session should start from `rip 0xffff8000010702a7`
  (`addr2line` → `kernel_core::process::wait4`) and the context that it lands
  around `exit` from the login shell, not from "TLS is broken" — which is what
  `SMOKE FAIL (net tls udp)` reads as, and is not.

### The next session should start here

1. **Phase 1, `kernel-core/src/graphics/kms.rs`** (298/300). This is the
   M13-5 prerequisite and `docs/PROGRESS.md` says "split it before extending".
   `tools/line-budget.txt` needs no edit — 298 is not over the line. Target
   layout and the reservation for M13-5's `ioctl.rs`/`connector.rs` are in §2
   of the batch plan.
2. Then `kernel/src/main.rs` (418 → boot/wiring; **delete its budget line**),
   `kernel-core/src/process.rs` (911 → six modules; **delete its budget
   line**), `kernel-core/src/vfs/mod.rs` (300/300), then the fdt and uart
   de-duplication.
3. Every split is a pure move; because this project has no unit tests, the
   evidence is that the same smoke assertion holds before and after. The
   suites that cover each split are listed in §2.

### Still open, in rough priority

- `iso` preload redesign (above) — blocks a fully green `make test-matrix`.
- `wait4` `#DB` (above).
- `smoke-iso` and the other QEMU suites are still absent from CI, which is why
  finding 24 sat undetected since P3. The cheapest honest mitigation remains
  the one in the batch plan's Phase 3: an `ALL_SMOKE_SET` + `make test-all`,
  and a dated full-matrix line in `PROGRESS.md` so "last verified" is a fact
  rather than a memory.
- Finding 25 (`check-toolchain.sh` 's narrow OVMF list, and its x86-only
  probes) is a small, self-contained fix.
- The NUL byte in the riscv CPU-model string (§3) is harmless under GNU grep
  but would bite any tooling that treats the log as binary.

### What the next reader should know about the numbers

`docs/PROGRESS.md` is the authority for milestone state and this file is not.
Where this file states a count (files over 300 lines, suites passing), it was
measured at the commit above and will drift; `tools/line-budget.txt` owns the
line-count list, and `build/baseline/RESULTS.txt` owns the baseline table.
