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

Kernel-side work (F3/F4): demand paging, COW, a kernel heap for page-table
and process structures, a VMA list per process, `fork/exec` cloning of
address spaces, `tmpfs`, a minimal `/dev` (null/zero/tty/fb/input) and
`/proc` (self/status/mounts).

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
| M14-8 | POSIX shell: bash over the native ABI (default `sh`, separate GPLv3 program with its sources); the built-in shell keeps the rescue builtins |

## 7.1 bash early start (C5, 2026-09)

Bash is already vendored (`apps/bash/`, GNU Bash 5.3, GPLv3, complete
sources), gated on `requires = ["posix-libc"]` and **does not run yet**. C5
started the real port work so M14-4/M14-8 begin from evidence:

- `apps/bash/port/REQUIREMENTS.md` - the minimal libc/POSIX surface
  (startup/`fork`/`execve`/`wait4`, pipes, signals, termios/job control,
  `getpwnam`, `glob`, locale/time stubs, ...);
- `apps/bash/port/README.md` - the exact configure/host flags attempted
  against `x86_64-unknown-none` (`--without-bash-malloc --disable-nls
  --without-readline --enable-static-link`) and the result: configure stops
  at `cannot compute sizeof (size_t)`, 39/43 probed headers and 102/102
  probed POSIX symbols are missing;
- `tools/build-bash-spike.sh` - reruns the cross-build attempt and writes
  `build/bash-spike/blockers.txt` deterministically (no network;
  `BASH_SPIKE_STRICT=1` fails while blocked).

**M14-4** must port musl over the native ABI (Track A syscalls + tmpfs from
§4) and re-run the spike; the header/symbol counts dropping to the musl set
is the exit criterion. **M14-8** then adds the replayable `patches/`, builds
`/usr/bin/bash` and flips the default `sh`; until then the kernel's built-in
shell and the interim `ping`/`nslookup`/`wget` bridge remain.

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
