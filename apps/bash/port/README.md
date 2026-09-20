# bash port — early-start record (C5)

The real port work for GNU Bash 5.3 (M14-8) starts here. Bash is vendored in
`apps/bash/` (C3) and stays unbuilt until the M14-4 POSIX/libc layer exists;
this directory records the cross-build attempt, the exact flags and the
blocker list so M14 starts from evidence instead of guessing.

**Status: bash does not run.** The spike is expected to fail until M14-4.

## Spike

```
tools/build-bash-spike.sh
```

It extracts the pristine tarball into `build/bash-spike/`, attempts the
configure below and probes the target's headers and POSIX symbols. It exits 0
once the blocker report is produced (`BASH_SPIKE_STRICT=1` exits 1 while
blocked, for M14 CI). Outputs: `build/bash-spike/{configure.log,
missing-headers.txt, missing-symbols.txt, blockers.txt}`.

## Exact flags attempted (2026-09, C5)

Configure (from `bash-5.3/`):

```sh
./configure \
  --host=x86_64-unknown-none \
  --build=x86_64-pc-linux-gnu \
  --without-bash-malloc \
  --disable-nls \
  --without-readline \
  --enable-static-link
```

Cross compiler (the same freestanding clang used for the kernel C layers):

```sh
clang --target=x86_64-unknown-none -ffreestanding -nostdinc \
      -isystem "$(clang -print-resource-dir)/include"
AR=llvm-ar RANLIB=llvm-ranlib
```

The host triple is the kernel's C target; `--without-bash-malloc` makes bash
use the libc allocator (the M14 plan's choice), `--enable-static-link` matches
the base-image policy, `--disable-nls`/`--without-readline` are the planned
M14-8 defaults. `BASH_SPIKE_TARGET` switches to another bare target (e.g.
`riscv64-unknown-none-elf`); the i686 path is
`targets/i686-fantuan-none.json` plus the nightly toolchain.

## Result (first run)

- configure exits 77 at
  `configure: error: cannot compute sizeof (size_t)` — the compile probes get
  far enough to fail on types because there is no libc header set.
- 39 of 43 probed headers are missing (all except the compiler-provided
  `stdarg.h`, `stddef.h`, `stdint.h`, `limits.h`).
- 102 of 102 probed POSIX symbols are undefined (`fork`, `execve`, `waitpid`,
  `pipe`, `termios`, `getpwnam`, `glob`, `locale`, ...).

The full lists are `build/bash-spike/missing-headers.txt` and
`build/bash-spike/missing-symbols.txt`; `apps/bash/port/REQUIREMENTS.md`
groups them into the minimal surface to implement/target first.

## Migration plan

1. **M14-2/M14-3**: native syscalls (`open/read/write/stat`, `fork/execve/
   wait4`, pipes, signals, `futex`) + `tmpfs` from `M14_LINUXUSERS.md` §4.
2. **M14-4**: musl port over the native ABI; rerun the spike with the musl
   sysroot (`--host=x86_64-fantuan-none`); the header count must drop to the
   musl set and the symbols resolve.
3. **M14-8**: replayable `patches/` (job control/pty defaults, terminal
   fallbacks), build `/usr/bin/bash`, choose the `sh` link and flip
   `CONFIG_APP_BASH` once `posix-libc` is available. The tools' catalog
   skeleton (`apps/{ping,nslookup,wget}`) migrates on the same layer.
