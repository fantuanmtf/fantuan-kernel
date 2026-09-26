# M14 Design — Linux Userspace, Self-Bootstrap, Hypervisor V2

> Status: design of record for v0.1.0, written before code (DESIGN §13.5).
> Roadmap: `ROADMAP_v0.0.2+.md` §6. This milestone unlocks M15 (XFCE/Qt)
> and the in-system toolchain.

## 1. Goal

Run a POSIX userspace on fantuan so that:

1. a compiler chain can be **bootstrapped in-system** (seed -> self-host ->
   build other tools), and
2. desktop and GUI software (M15) has the platform it needs: processes,
   virtual memory, signals, sockets, shared memory.

## 2. Non-goals

- Running unmodified distro binaries in v0.1.0 (that is the optional compat
  track C, below).
- C++ self-bootstrap: impossible from cc+nasm; the C++ seed ships prebuilt
  (recorded exception).
- Replacing the existing native syscall ABI: it grows, it is not replaced.

## 3. Tracks

| Track | What | When |
|---|---|---|
| A. Native syscalls (primary) | grow the fantuan syscall set to a POSIX-shaped surface; port musl; compile all base software from source | v0.1.0 |
| B. Toolchain bootstrap (F7) | seed tcc/nasm -> self-host -> build tools/sources in-system | v0.1.0 |
| C. Linux compat (optional) | Linux ELF loader + syscall translation so unmodified binaries run | v0.2.x, only if needed |

Track A is what makes the "kernel compiles its own userspace" story real;
Track C is rescue value, not a dependency.

## 4. POSIX syscall surface (Track A)

The native ABI (currently SYS_VERSION..SYS_YIELD) gains, in append-only
order (numbers never renumber):

- memory: `mmap`/`munmap`/`mprotect`, `brk`;
- files: `open`/`close`/`read`/`write`/`lseek`/`stat`/`fstat`/`getdents`,
  `mkdir`/`unlink`/`rename`, `dup`/`dup2`, `pipe`;
- processes: `fork`, `execve`, `wait4`, `exit`, `getpid`, `kill`;
- signals: `sigaction`/`sigprocmask`/`sigreturn` (a small, well-defined set);
- time: `clock_gettime`, `nanosleep`;
- sync: `futex` (wait/wake for musl);
- network (from M11): `socket`/`bind`/`connect`/`sendto`/`recvfrom`;
- misc: `uname`, `getcwd`/`chdir`, `ioctl` (framebuffer/input from M13).

**P1 first tranche (landed 2026-09):** `open/close/read/write-on-fd/lseek/
stat/fstat/getdents/brk/pipe/dup/dup2/ioctl/clock_gettime/getpid/getppid/
chdir/getcwd/unlink/mkdir/rmdir/rename` + a writable tmpfs and
`/dev/console`/`/dev/null`; the proof is `tools/smoke-posix.sh` (a C
hello runs, exits and is reaped).

**P2 (landed in the working tree, 2026-09):** `fork/execve/wait4`, process
groups/sessions, `sigaction/sigprocmask/sigreturn/sigsuspend`, the VMA
list with anonymous `mmap`/`munmap`/`mprotect`, a console line discipline
and the append-only ABI additions (`SYS_FORK 29`..`SYS_FCNTL 47`). dash
0.5.12 (BSD-3-Clause) is vendored, cross-built against libc-fantuan and
embedded; in P2 it became the interim default `sh`, with first-party
`/bin/ls` and `/bin/cat`; the built-in kernel shell remains the boot
console and rescue fallback. `tools/smoke-dash.sh` is the gate; the fault
root causes and the exact limits (no job-control stop, no file-backed
mmap) are recorded in `docs/POSIX_PLAN.md` P2 outcome.

**P3 (landed in the working tree, 2026-09):** the libc surface bash needs
is complete (spike: 0/43 headers, 102/102 symbols) and bash 5.3 is
cross-built by `tools/build-bash.sh` against libc-fantuan. **`sh` is now
bash** (`/bin/sh`, `/bin/bash`, the `sh`/`bash` console commands), dash
stays `/bin/dash` and the `dash` command keeps it selectable, and
`tools/smoke-bash.sh` proves `-c`, interactive sessions, arithmetic,
variables, functions, pipes, redirects, `$(...)`, `^C` and the reaps.
The kernel/loader fixes the static-bash load forced (page-walk ELF
loading, zeroed page-table frames) are in `docs/POSIX_PLAN.md` P3
outcome. `futex` and job-control stop/continue remain for P4.

Kernel-side work (F3/F4): demand paging, COW, a kernel heap for page-table
and process structures, a VMA list per process, `fork/exec` cloning of
address spaces, `tmpfs`, a minimal `/dev` (null/zero/tty/fb/input) and
`/proc` (self/status/mounts). **P1/P2 status:** the tmpfs, the minimal
`/dev` (null/console) and the per-process VMA list (anonymous mappings,
eager copy on fork) exist; demand paging, COW and the kernel heap remain
for M14-1.

## 5. Toolchain bootstrap chain (F7)

| Stage | Runs on | Produces | Notes |
|---|---|---|---|
| 0. Seed | image build (host cross) | `tcc` (x86_64 + i686 + arm64 backends) and `nasm` binaries; musl; bmake; bash (GPL, separate program with its sources) | the only cross-built links in the base image |
| 1. Self-host | target | tcc rebuilds tcc from source | proves reproducibility; hash recorded |
| 2. Tools | target | tcc builds NASM; bmake drives builds | NASM is BSD-2 |
| 3. Userspace | target | C sources (apps-catalog tools, Xorg, XFCE C parts) compiled on target | C++ handled by the seed below |
| 1C. C++ seed | developer image (host cross) | clang (C/C++) for the target | ships prebuilt; documented exception |

License hygiene for the base system: tcc (LGPL) and nasm (BSD-2) are *tools*
invoked as programs; base userland prefers permissive code from the apps
catalog (BSD/0BSD implementations) instead of busybox (GPL), **bmake
(BSD)** or ninja (Apache-2.0) instead of GNU make, musl (MIT). bash is
the one registered GPLv3 program (the default shell) and ships as a
separate program with complete corresponding sources. Copyleft desktop packages (XFCE, Qt) are
optional developer-image programs aggregated with their sources, never
linked into the kernel. `THIRD_PARTY.md` records every component, version,
license and source location.

## 6. Hypervisor V2 (F8)

Scope for v0.1.0 (no device passthrough yet):

- **Intel**: VMXON/VMCS setup, EPT identity + guest memory, basic exit
  handling (CPUID, HLT, EPT violations, MMIO), guest serial via a simple
  emulated UART;
- **AMD**: SVM/VMCB equivalent to the above;
- **Guest lifecycle**: create, run, stop, destroy one guest; host remains in
  control of interrupts and timers;
- **Guest target**: a minimal Linux kernel + initramfs provided by the image
  builder (the image ships it as a payload, not as source).

V3 (M15) adds the virtio disk backend, the disk-service guest and the VM
console window; the Qubes-like isolation claim and its IOMMU requirements
are documented then, in the threat model.

## 7. Work breakdown (suggested commits)

| Step | Deliverable |
|---|---|
| M14-1 | Kernel heap + VMA list + demand paging + COW (no syscalls yet; kernel tests) |
| M14-2 | POSIX round 1: mmap/brk/open/read/write/stat/getdents; tmpfs; /dev/null/zero |
| M14-3 | POSIX round 2: fork/execve/wait4, signals, futex, pipes |
| M14-4 | musl port + bmake + the first apps-catalog tools boot under the native ABI |
| M14-5 | Seed/tcc/self-host chain + `/bootstrap.sh` + reproducibility hash |
| M14-6 | C++ seed (clang) in the developer image; build one C++ program |
| M14-7 | Hypervisor V2 (VMX first, SVM second) + guest serial + docs/threat model stubs |
| M14-8 | POSIX shell: bash over the native ABI (**the login shell** since P4, and the default `sh`; separate GPLv3 program with its sources); the built-in shell keeps the rescue builtins |

## 7.1 bash early start (C5, 2026-09)

Bash is already vendored (`apps/bash/`, GNU Bash 5.3, GPLv3, complete
sources), gated on `requires = ["posix-libc"]` and **does not run yet**. C5
started the real port work so M14-4/M14-8 begin from evidence:

- `apps/bash/port/REQUIREMENTS.md` - the minimal libc/POSIX surface
  (startup/`fork`/`execve`/`wait4`, pipes, signals, termios/job control,
  `getpwnam`, `glob`, locale/time stubs, ...);
- `apps/bash/port/README.md` - the exact configure/host flags
  (`--without-bash-malloc --disable-nls --disable-readline
  --enable-static-link`) and the before/after result: the earlier
  `cannot compute sizeof (size_t)`, 39/43 headers and 102/102 symbols
  missing, now 0/43 headers and 0/102 symbols missing (P3);
- `tools/build-bash-spike.sh` - reruns the cross-build attempt and writes
  `build/bash-spike/blockers.txt` deterministically (no network;
  `BASH_SPIKE_STRICT=1` fails while blocked). The optional
  `BASH_SPIKE_LIBC_INC`/`BASH_SPIKE_LIBC_A` probe measures the target
  libc: with P1 libc-fantuan configure exited 0 (13/43 headers and
  25/102 symbols still missing); with P3 the report is 0/43 and 0/102 -
  see `docs/POSIX_PLAN.md` P3 outcome.

**P1 (2026-09)** landed the first POSIX tranche (`docs/POSIX_PLAN.md`):
the v2 syscalls, a writable tmpfs with `/dev/console`/`/dev/null`,
`libc-fantuan` (MIT), and a C hello that runs, prints, exits and is reaped
under the existing ELF loader (`tools/smoke-posix.sh`). **P2 (2026-09)**
then landed the process/signal/VMA layer and the dash port; the `sh`
command ran dash (interactive or `-c`/file) and `tools/smoke-dash.sh`
asserted the transcripts, statuses and reaps; the built-in shell is the
boot console/rescue fallback. **P3 (2026-09) finished the port: bash 5.3
builds against libc-fantuan (`tools/build-bash.sh`, one replayable patch,
`-nostdlib` probes, byte-reproducible 743,312-byte artifact) and is the
default `sh`; dash stays `/bin/dash` and selectable. `tools/smoke-bash.sh`
asserts `-c`, interactive use, arithmetic/variables/functions, pipes,
redirects, `$(...)`, `^C` and the reaps. M14-8 is therefore closed at the
shell level; the spike now reports 0/43 missing headers and 102/102
probed symbols (M14_LINUXUSERS §7.1). **M14-4** decides musl vs
libc-fantuan per the P4 triggers, and the remaining shell follow-ups (pty,
job-control stop/continue) stay on the M14/P4 backlog; the built-in shell
and the interim `ping`/`nslookup`/`wget` bridge remain.

**M14-8 follow-up — file-based bash delivery (added by the licensing
refactor).** Today bash's program is embedded in the kernel image as an
opaque blob (`include_bytes!`), so the image is a distribution of GPLv3 bash
and the source provision of `THIRD_PARTY.md` applies to it. M14 replaces the
embedded payload with file-based delivery: ship the bash ELF (and `dash`)
through the filesystem the kernel grows (initrd/tmpfs/ESP) at `/bin`, and
ship the complete corresponding sources in the image next to it at
`/usr/src/bash/` (`bash-5.3.tar.gz` + `.sig` + `COPYING` + `SOURCE` + the
patch and the build recipe), so the running system and any image copy carry
what GPLv3 §6 requires without embedding anything in the kernel. Until then
the embedding is deliberate and recorded, and `tools/smoke-gpl.sh` asserts
both halves of the truth (not linked, but embedded).

## 8. Spikes before coding

- musl TLS/thread-pointer requirements on a new ABI (and `futex` semantics).
- COW correctness under fork+exec with demand paging.
- TCC self-build determinism and its C99 coverage on our target.
- VMX bring-up on QEMU (nested) and on one real Intel host; VM-exit budget
  and interrupt-injection basics.

## 9. Verification

- Bootstrap: `/bootstrap.sh` produces binaries byte-identical across two
  runs (hash recorded in the smoke log).
- Userspace: apps-catalog commands (bash as the default shell), a shell
  script suite, fork/exec/pipe tests, musl's own test subset where
  feasible.
- Hypervisor: guest Linux boots to the initramfs under VMX on QEMU nested
  and on one bare host; the host survives guest faults and shutdowns.
- All earlier smokes (x86_64 UEFI/BIOS, i686, riscv64, arm64) stay green.

## 10. Risks

| Risk | Mitigation |
|---|---|
| POSIX surface grows without bound | fixed v0.1.0 list above; additions are append-only and need a consumer |
| musl port stalls on TLS/futex details | spike first; fall back to a tiny libc for the seed tools if needed |
| C++ seed size vs the 4 GB image cap | clang in the developer image only; source fetched at runtime |
| Hypervisor complexity | V2 is deliberately minimal (one guest, no passthrough); V3 is a separate milestone |
| Copyleft contamination claims | license policy in ROADMAP §0; THIRD_PARTY.md per component |
