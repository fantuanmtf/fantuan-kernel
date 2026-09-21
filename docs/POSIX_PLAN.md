# POSIX plan (P batches): libc-fantuan -> dash -> bash

> Status: **P2 landed in the working tree (2026-09): dash runs and `sh` is
> dash**; P3 (bash) is next. This is the staged execution ledger for M14
> track A (`docs/M14_LINUXUSERS.md` §4/§7, `docs/APPS.md` bash exception).
> The built-in kernel shell stays the boot console / rescue fallback.

## Batches

| Batch | Scope | Shell outcome |
|---|---|---|
| **P1** | Native ABI v2 (round 1) + writable tmpfs + `/dev/console`, `/dev/null` + `libc-fantuan` + the first C user program + `tools/smoke-posix.sh` | none; a C program runs through the ELF loader |
| **P2** | `fork`/`execve`/`wait4`, process groups, signals, the real `mmap`/VMA path, per-ELF heap base, line-discipline input; port **dash** (BSD-2/3) as `/bin/dash`, then flip the default `sh` to it | a real POSIX shell, permissive only |
| **P3** | Port **bash 5.3** (GPLv3 registered exception, `apps/bash/`): glob/fnmatch/wordexp, regex, locale/time stubs, wide chars, pwd/grp completeness, termios/job control, replayable `patches/`; ship `/usr/bin/bash` with sources in the image | bash as a separate program; built-in shell keeps the rescue builtins |
| **P4** | musl vs libc-fantuan decision (M14 §5): if libc-fantuan stalls on TLS/futex/locale, port musl (MIT) over the same native ABI - the syscall layer is the stable contract | unchanged shell binary |

## P1 - what landed

**ABI (append-only, numbers never renumber).** `SYS_OPEN 6` through
`SYS_RENAME 28` (`abi/src/lib.rs`): open/close/read/write-on-fd/lseek/stat/
fstat/getdents/brk/mmap(stub)/pipe/dup/dup2/ioctl/clock_gettime/getpid/
getppid/chdir/getcwd/unlink/mkdir/rmdir/rename. `SYS_WRITE` (3) keeps its v1
`(buf, len)` debug-channel semantics forever; fd writes use `SYS_WRITE_FD`
(9). `SYS_VERSION` still reports `ABI_VERSION = 1`: the additions are
additive, so v1 consumers keep working. Payload structs (`Stat`, `Timespec`,
`Dirent`, `Termios`, `Winsize`) live in `abi/src/posix.rs` and are mirrored
by libc headers. Errors are negative **Fantuan-native** errno values (not
Linux's; `libc-fantuan/include/errno.h`).

**Kernel side.** `kernel-core/src/vfs/tmpfs.rs` (fixed static tables: 32
nodes, 8 files x 8 KiB), `vfs/fd.rs` (16 fds/task, 64 opens, 8 pipes x
512 B, refcounted dup), `vfs/posix.rs` (user copies + path syscalls),
`brk.rs` (per-task heap at `USER_HEAP_BASE` = 0x500000), `user::UserMemOps`
(the x86_64 stac/clac copy bridge, `kernel/src/arch/x86_64/syscall.rs`).
`spawn_user_args` builds the SysV `argc/argv/envp` stack
(`kernel/src/task/mod.rs`). The user program's heap state and fd table are
reset at task registration and closed at exit/reap.

**Filesystem (what is writable vs read-only).** `/` and `/tmp`, `/etc`,
`/bin`, `/usr` are writable tmpfs directories; `/dev/console` and
`/dev/null` are fixed character nodes (mkdir/unlink/rename cannot touch
nodes 0..7). Nothing persists across reboot, deleting a file does not
reclaim its pool chunk, and the FAT/ext4 disk mounts from M6 stay
**read-only** (`/mnt/disk0`, `/mnt/root0`); the P2 shell will need the tmpfs
(or a future writable root) for `/tmp`.

**libc-fantuan** (`libc-fantuan/`, MIT, first-party): headers for the P1
scope plus `sys/time.h`, `sys/resource.h`, `sys/select.h`, `sys/times.h`,
`sys/ioctl.h`, `poll.h`, `strings.h`, `pwd.h`, `inttypes.h`, `paths.h`,
`sys/file.h`, `sys/param.h`; implementations for the syscall wrappers,
malloc over brk, string/memory, buffered stdio + a full printf core,
dirent, time, termios, ctype, pwd/grp, and `src/stubs.c` (ENOSYS). Built by
`tools/build-libc.sh` (deterministic, `--verify`), x86_64 first.

**Proof.** `tools/smoke-posix.sh` boots the **minimal** kernel (no
tools/net) with `kernel/hello_program.bin` (from `user/hello.c`) embedded:
the C program prints argv, mallocs over brk, round-trips a tmpfs file,
pipes `write=4 read=4`, reads the clock, exits 0, and the kernel reaps it
(`sched: reaped tid N`). `docs/M14_LINUXUSERS.md` §7.1 records the step.

## P1 honest limits (carried into P2)

- No `fork`/`execve`/`wait4`, no signals, no process groups; `getppid` is 0.
- `mmap` returns ENOSYS (the brk allocator covers the P1 programs).
- `getdents` returns one Fantuan-native 72-byte record per call.
- Console reads return EOF (no line discipline/input path yet); `ioctl`
  reports ICANON/ECHO/ISIG and 80x25 but stores nothing.
- Static caps: 16 fds/task, 64 opens, 8 pipes, 8 files x 8 KiB, 32 nodes.
- Heap base is fixed at 0x500000, not derived from the ELF end.
- No permission/security model; everything runs as root.
- x86_64 only; riscv/i686/aarch64 keep the v1 debug channel.

## P2 - dash (concrete blockers)

From the C5 spike (`tools/build-bash-spike.sh`, recorded in
`apps/bash/port/`) and the dash requirements:

1. **Process model**: `fork`, `execve`, `wait4` (with `WUNTRACED`),
   `setpgid`, `getpgrp`, `kill`/`killpg`, zombie reaping, and an `exec`
   path that loads a second ELF into the same task's address space.
2. **Signals**: a real `sigaction`/`sigprocmask`/`sigreturn` set with
   `SIGINT`/`SIGCHLD`/`SIGQUIT`/`SIGTERM` delivery and `wait` interruption.
3. **Input**: console read path (PS/2 + serial ring), `tcgetattr` line
   discipline, `SIGTTIN`/`SIGTTOU`, so `dash -i` is usable.
4. **libc**: `setjmp.h` (real), fork/exec/wait wrappers, `system`/`popen`,
   a per-ELF heap base, `mmap` for large blocks.
5. **New syscalls**: fork/execve/wait4/kill/sigaction/sigprocmask/
   sigreturn/setpgid/getpgid/getpgrp/getppid(real)/nanosleep(real)/
   mmap/munmap, plus `dup2`-based redirection (already present).
6. **Heap/COW**: fork needs address-space cloning (COW or copy); the M14-1
   VMA/COW work is the prerequisite.

## P3 - bash (concrete blockers)

Measured with the libc include/archive
(`BASH_SPIKE_LIBC_INC=libc-fantuan/include`, `BASH_SPIKE_LIBC_A=...`):

| Metric | C5 (bare) | P1 (libc-fantuan) |
|---|---|---|
| configure | `cannot compute sizeof (size_t)` | exits 0 (Makefiles generated) |
| missing headers | 39 of 43 | **13 of 43** |
| probed symbols resolved | 0 of 102 | **76 of 102** (62 real + 14 ENOSYS stubs) |

Remaining **13 headers**: `setjmp.h`, `sys/socket.h`, `grp.h`, `wchar.h`,
`wctype.h`, `glob.h`, `locale.h`, `langinfo.h`, `iconv.h`, `wordexp.h`,
`dlfcn.h`, `libintl.h`, `regex.h`.

Remaining **26 symbols**: `setpgid`, `forkpty`, `openpty`, `glob`,
`globfree`, `fnmatch`, `wordexp`, `wordfree`, `regcomp`, `regexec`,
`regfree`, `setlocale`, `localeconv`, `nl_langinfo`, `mbrtowc`, `wcrtomb`,
`mbsrtowcs`, `wcsrtombs`, `iconv_open`, `iconv`, `iconv_close`, `popen`,
`pclose`, `dlopen`, `dlsym`, `dlclose`.

Plus the P2 process/signal layer (bash needs job control, `tcgetpgrp`/
`tcsetpgrp`, `killpg`, `SIGCHLD` traps), `sigsetjmp`/`siglongjmp`, and the
`--without-readline --without-bash-malloc --disable-nls --enable-static-link`
port path already pinned in `apps/bash/port/README.md`. The 14 stubs among
the 76 (`fork`, `execve`, `waitpid`, `readlink`, `symlink`, `tcgetpgrp`,
`tcsetpgrp`, `signal`, `sigaction`, `sigprocmask`, `sigsuspend`, `kill`,
`killpg`, `system`) must become real before any shell runs.

## P4 - musl vs libc-fantuan

M14 §5 prefers musl (MIT) for Track A once the syscall surface stabilises.
Decision triggers, in order: (a) libc-fantuan cannot carry bash's locale/
wide-char/regex surface without becoming a second musl; (b) TLS/thread
requirements appear (bash 5.3 has no thread dependency on this path, so
this is unlikely); (c) the in-system toolchain (M14-5) needs a libc it can
rebuild from source. Until then libc-fantuan stays the P2/P3 base because it
is small, first-party, permissive, and its failures are visible. Either way
the **ABI is the contract**: musl would port over the same v2 calls.

## P2 outcome (dash runs; verified 2026-09)

Delivered and verified by `user/proc_test.c` at boot: fork returns twice
(child exits 42), wait4 reaps with the right status, a pipe survives
fork, mmap-backed memory works, and execve replaces the image (the
"proc_test re-exec image" step). The process layer (process.rs), signal
delivery (signal.rs) and a console line discipline (tty.rs) landed in
kernel-core, the ABI grew append-only (fork/execve/wait4/mmap/...), and
libc-fantuan gained the matching headers and stubs.

dash 0.5.12 is vendored in `apps/dash/` (BSD-3-Clause, gpl=false,
requires posix-libc) and `tools/build-dash.sh` cross-builds it against
libc-fantuan into a ~160 KB ELF that the kernel embeds. `tools/smoke-dash.sh`
boots the minimal kernel and drives a paced feeder through both sessions.

**The original "dash #PF at 0x400b85" report was a misdiagnosis.** That
fault is the deliberate ring-3 fault test in the M4 Rust demo
(`user/src/main.rs` writes `0x500000`); it fires before `sh` runs and is by
design. dash itself never faulted. The defects that actually blocked the
shell, in the order they were found:

1. **Job-control probe spin (kernel + libc).** dash's `setjobctl()` does
   `do { pgrp = tcgetpgrp(fd); if (pgrp == getpgrp()) break;
   killpg(0, SIGTTIN); } while (1)`. The `sh` command spawned dash with no
   console foreground group (`TTY_PGRP` 0) and libc's `killpg(0, sig)`
   returned `EINVAL` instead of signalling the caller's own group, so dash
   spun forever before reading a byte. Fix: `sh` hands dash the terminal
   (`process::set_tty_pgrp(child)`, 0 restored after the wait) and
   `killpg(0, sig)` maps to `kill(0, sig)`.
2. **Decimal conversions (libc).** `strtoull` left `base = 0` when the
   string had no `0x`/leading-`0` prefix, so the digit loop rejected every
   decimal digit; `$((1+2))` failed with `arithmetic expression: expecting
   EOF: "1+2"` and `x=4` was `Illegal number: 4`. Fix: base 0 infers 10
   (8 with a leading 0, 16 with `0x`) and a lone `0`/`0x` consumes the
   correct characters.
3. **`getdents` length (libc).** `readdir` passed `len = 0` to
   `SYS_GETDENTS` (the kernel requires >= the 72-byte record), so any
   directory read returned `EINVAL`. Fix: pass `sizeof(struct dirent)`.
4. **No `/bin` entries (kernel).** dash resolves a PATH command by
   `stat`ing the candidate before `execve`; the embedded ELFs existed only
   in the `lookup_bin` registry, so `ls`, `cat` and `sh` were "not found".
   Fix: the VFS gained a read-only registry overlay - `stat`, `open`,
   `read`, `fstat` and `lseek` answer `/bin/<name>` from the embedded
   images (`vfs/{fd,io,dir,posix_path}.rs`) - plus first-party
   `user/ls.c`/`user/cat.c` tools so dash can fork/exec a real pipeline.
5. **Zombie/sleep scheduler freeze (kernel).** Blocking syscalls run with
   IF=0 (INT 0x60 is an interrupt gate) and the sleep clock was the
   IRQ-driven PIT counter: when a child exited while every waiter slept,
   `task::exit_with` spun in `schedule()` with interrupts disabled and no
   tick could advance (symptom: `cat` followed by `^C` hung the machine).
   Fix: `exit_with` enables IRQs and idles behind a new
   `arch::set_irq_enable` hook, and x86_64 `now_ticks` reads the
   calibrated TSC in 100 Hz units (floored by the PIT count) so wakeups
   keep working with IF=0.

**Proven by `tools/smoke-dash.sh`** (minimal profile): `sh -c '<script>'`
with a clean and a non-zero exit (`wait status=0x700`); interactive dash
on `/dev/console` with prompt, tty echo/erase, `^C` -> SIGINT
(`$?` = 130) and `exit` back to the built-in shell; command substitution,
`$((1+2))`, pipelines (`echo | cat`), `>`/`<` redirections, scripts by
file (`sh /tmp/s.sh`), `$?`, fork/exec/wait and the reaps.

**What "default sh" means now (exact scope).** The `sh` console command
launches the embedded dash 0.5.12 (`/bin/dash`, BSD-3-Clause, linked
against libc-fantuan) and passes arguments through (`sh -c ...`,
`sh FILE`); `execve("/bin/sh")` and `execve("/bin/dash")` resolve to the
same image. The built-in kernel shell remains the boot console and the
rescue fallback (its command set is still gated by `CONFIG_RESCUE_REPAIR`/
`CONFIG_TOOLS`); when the dash artifact is absent the `sh` command says so
and the built-in shell is all there is. bash stays the M14-8 default-shell
target for P3.

**P2 limits (still unsupported in dash).** No job-control stop/continue
(`^Z` discards the line; `SIGTSTP`/`SIGTTIN` default to ignore), no
controlling-terminal enforcement (any task may `tcsetpgrp`), mmap is
anonymous-private only (no file-backed/shared mappings), the `/bin`
registry entries are stat/open/exec-visible but not `getdents` entries
(`ls /bin` is empty), getdents still returns one 72-byte record per call,
tmpfs is 8 files x 8 KiB and pipes are 8 x 512 B. None of these block
dash; they are the P3/P4 backlog.
