# bash port record (C5 -> P3)

GNU Bash 5.3 (M14-8) is vendored in `apps/bash/`. C5 recorded the bare-target
spike; P1/P2 grew `libc-fantuan` and the process layer; **P3 (2026-09) closed
the gap: bash builds and runs as `/bin/bash`, and `sh` prefers it.**

`REQUIREMENTS.md` holds the minimal libc/POSIX surface; this file holds the
measured state and the exact build recipe.

## Spike: blocker report before/after

`tools/build-bash-spike.sh` extracts the pristine tarball, attempts configure
and probes 43 headers / 102 POSIX symbols against the target libc.

| Metric | C5 (bare) | P1 (libc-fantuan) | P3 (landed) |
|---|---|---|---|
| configure | `cannot compute sizeof (size_t)` | exits 0 | exits 0 |
| missing headers | 39 of 43 | 13 of 43 | **0 of 43** |
| symbols the archive provides | 0 of 102 | 76 of 102 | **102 of 102** |
| remaining symbols | 102 | 26 | **0** |

P3 added the 12 headers (`setjmp.h` landed in P2): `sys/socket.h`, `grp.h`,
`wchar.h`, `wctype.h`, `glob.h`, `locale.h`, `langinfo.h`, `iconv.h`,
`wordexp.h`, `dlfcn.h`, `libintl.h`, `regex.h`. The 25 remaining symbols
(`glob`/`globfree`, `fnmatch`, `wordexp`/`wordfree`, `regcomp`/`regexec`/
`regfree`, `setlocale`/`localeconv`/`nl_langinfo`, `mbrtowc`/`wcrtomb`/
`mbsrtowcs`/`wcsrtombs`, `iconv_*`, `popen`/`pclose`, `dl*`, `forkpty`/
`openpty`; `setpgid` had landed in P2) are implemented or honest stubs in
`libc-fantuan` (see `libc-fantuan/README.md`).

## Build configuration (tools/build-bash.sh)

```
clang --target=x86_64-unknown-none -ffreestanding -nostdinc \
      -isystem "$(clang -print-resource-dir)/include" \
      -I libc-fantuan/include -std=gnu11 -O2
configure --host=x86_64-unknown-none --build=x86_64-pc-linux-gnu \
          --without-bash-malloc --disable-nls --disable-readline \
          --enable-static-link
LIBS="libc-fantuan/obj/crt0.S.o libc-fantuan/libc-fantuan.a"
CFLAGS_FOR_BUILD="-g -DCROSS_COMPILING -std=gnu11"   # host generators
bash_cv_func_strchrnul_works=yes bash_cv_getcwd_malloc=yes
link: LDFLAGS="-nostdlib -static -no-pie -Wl,--build-id=none -Wl,-s -Wl,--gc-sections"
```

Notes:

- `-nostdlib` + the libc archive in `LIBS` make configure's link probes
  resolve against `libc-fantuan`, so `HAVE_*` reflects the target, not the
  host glibc (the C5 spike accidentally linked host glibc).
- `bash_cv_func_strchrnul_works` / `bash_cv_getcwd_malloc` are cross-run
  answers; without them bash builds its own `strchrnul`/`getcwd` and collides
  with libc's.
- `--disable-readline` (bash has no `--without-readline`) keeps the readline
  library out; history stays bash-internal.
- The vendored source is patched once: `patches/0001-netopen-no-network-decls.patch`.
- Result: `build/bash/bash.elf`, 743,312 bytes stripped, sha256
  `38ec6a3028d89bbb879315707ba3a0274b6264345181a0693dc55bc94c65e5ac`
  (byte-reproducible; `BASH_VERIFY=1`), embedded as `kernel/bash_program.bin`.

## Runtime notes

`tools/smoke-bash.sh` proves `bash -c 'echo ...'`, `exit 7`, arithmetic,
variables, a function, a pipeline, a redirection, `$(...)`, an interactive
prompt with `^C` (status 130) and the reaps. `sh` prefers bash; dash remains
selectable (`/bin/dash`, the `dash` console command) and `smoke-dash.sh` is
kept green by selecting it explicitly.

Call-path work P3 needed beyond libc:

- `sigsetjmp` is a macro that expands at the call site (`setjmp` must belong
  to the caller's frame; a nested C `sigsetjmp` did not restore reliably).
- the ELF loader lost its fixed 64-page tracking table (a static bash is
  ~190 pages) and page-table frames are zeroed on allocation (a recycled
  frame's stale bytes read as present entries).
