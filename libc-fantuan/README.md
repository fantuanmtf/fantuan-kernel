# libc-fantuan (P1-P3)

A small freestanding C library for fantuan's native syscall ABI, MIT-licensed
and written for this kernel (not a port of an existing libc). P1 started it
with enough libc to run a C program through the ELF loader; P2 added the
process/signal layer (dash runs); P3 completed the surface bash needs (the
spike resolves all 43 probed headers and 102/102 probed symbols).

**What P3 is not:** locale databases and iconv tables (C locale only),
dynamic loading (`dl*` return NULL/ENOSYS), pty (`forkpty`/`openpty`
ENOSYS), a regex with back-references, or a full network stack
(`sys/socket.h` is link surface for configure). See `docs/POSIX_PLAN.md`
for the staged path and the P3 outcome.

## Build

```sh
tools/build-libc.sh
```

It compiles `libc-fantuan/src/*.c` and the two `.S` files with the kernel's
freestanding clang flags:

```
clang --target=x86_64-unknown-none -ffreestanding -nostdinc \
      -isystem "$(clang -print-resource-dir)/include" \
      -I libc-fantuan/include \
      -fno-builtin -fno-stack-protector -fno-pic -mno-red-zone \
      -fno-asynchronous-unwind-tables -fno-unwind-tables -O2 -Wall -Wextra -Werror
```

Outputs (under `build/libc-fantuan/`):

- `libc-fantuan.a` — deterministic static archive (`llvm-ar rcsD`);
- `hello.elf` — the P1 test program (`user/hello.c`), ET_EXEC at 0x400000;
- `kernel/hello_program.bin` — a copy the kernel `build.rs` embeds when
  present (a fresh default build without it is unchanged).

aarch64/riscv/i686 archives are deliberately not built yet (P1 is x86_64
first); the syscall layer is arch-neutral, so the crt and the syscall
trampoline are the only per-arch pieces.

## Design

- **ABI**: syscalls return `-errno` in the Fantuan-native numbering (see
  `include/errno.h`; not Linux's). The register ABI is the kernel's
  (`rax` number, `rdi/rsi/rdx/r10/r8` args, `int 0x60`), wrapped by
  `src/syscall.S`.
- **Startup**: `src/crt0.S` reads the SysV `argc/argv/envp` the kernel
  builds (`kernel/src/task/mod.rs`), calls `main` through
  `__libc_start_main`, then `exit`.
- **malloc**: a free-list over `brk` in 64 KiB chunks (`src/malloc.c`),
  coalescing adjacent frees; this is the allocator bash uses with
  `--without-bash-malloc`.
- **stdio**: FILE objects over the fd syscalls; stdout line-buffered,
  stderr unbuffered, stdin buffered; the printf core supports
  flags/width/precision/length and `d i u o x X p c s %`.
- **Files**: `open/read/write/lseek/stat/fstat/getdents/pipe/dup/dup2/
  ioctl/chdir/getcwd/unlink/mkdir/rmdir/rename` map 1:1 to the v2 calls.
  `/dev/console` and `/dev/null` are the stdio devices; `/tmp`, `/etc`,
  `/bin`, `/usr` are the writable tmpfs directories.
- **Time**: no RTC yet, so `CLOCK_REALTIME` == monotonic since boot and all
  calendar conversions are UTC.
- **Stubs**: unimplemented functions are present and set `ENOSYS` so
  configure-style probes link; `src/stubs.c` lists them.
- **P3 surface**: `fnmatch`/`glob` (small in-repo implementations),
  an original compact BRE/ERE regex (`regex_parse.c`, `regex_class.c`,
  `regex_compile.c`, `regex_exec.c`), UTF-8 `wchar`/`wctype`, C-locale
  `setlocale`/`localeconv`/`nl_langinfo`, `wordexp`, `popen`/`pclose` over
  `/bin/sh`, and honest ENOSYS link surface for `iconv_*`, `dl*`, sockets
  and `forkpty`/`openpty` (`src/nls_stubs.c`, `src/net_stubs.c`,
  `src/pty.c`). `sigsetjmp` is a call-site macro in `setjmp.h`; its
  `jmp_buf` must belong to the caller's frame.

## Layout

```
include/           public C headers (fantuan/abi.h is the internal mirror)
src/*.c, *.S       implementations, compiled in filename order
elf.ld             ET_EXEC 0x400000 linker script
LICENSE            MIT
```

Files stay <=300 lines (repo convention); sources are split accordingly
(`strings.c`, `stdio_file.c`, `printf_file.c`, `stdlib_misc.c`, `env.c`,
`wstring.c`, `regex_*.c`).
