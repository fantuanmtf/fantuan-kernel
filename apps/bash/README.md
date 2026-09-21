# bash

**GNU Bash 5.3** — the default POSIX shell (M14-8). Separate GPLv3 program,
registered in [THIRD_PARTY.md](../../THIRD_PARTY.md) and allowed by
`apps-catalog.toml` `[licensing] gpl_allow = ["bash"]`; see the app contract in
[docs/APPS.md](../../docs/APPS.md).

## What ships

| Path | Content |
|---|---|
| `src/bash-5.3.tar.gz` | pristine upstream release tarball (complete corresponding source) |
| `src/bash-5.3.tar.gz.sig` | upstream detached signature (verified against the GNU keyring) |
| `src/SHA256SUMS` | sha256 of the tarball (`sha256sum -c` in `src/`) |
| `src/SOURCE` | release URL, retrieval date, signature and provenance notes |
| `COPYING` | GPLv3 text, extracted from the tarball |
| `patches/` | replayable fantuan-ABI patches (`0001-netopen-no-network-decls.patch`; replayed by `tools/build-bash.sh`) |
| `port/` | port record: `README.md` (config + build recipe) and `REQUIREMENTS.md` (the minimal POSIX surface) |

## Source provision (GPLv3)

GPLv3 compliance is satisfied with the complete corresponding source next to
the binary: the repo carries the pristine tarball under `apps/bash/src/`
(pinned by `apps.lock` and `SHA256SUMS`), and image assembly copies it into
the shipped image at `/usr/src/bash/` (sources bundle) together with
`COPYING` and `SOURCE`. The build recipe (manifest + this README + `patches/`)
travels with the same tree, so a recipient can rebuild the exact binary.

## How it is built (P3, 2026-09)

**bash builds and runs.** P1/P2 provided `libc-fantuan` and the process
layer; P3 filled the remaining libc surface (fnmatch/glob, regex, locale,
wchar, wordexp, popen, iconv/dl/dlfcn stubs) and `tools/build-bash.sh`
cross-builds the pristine tarball for `x86_64-unknown-none`:

1. extracts `src/bash-5.3.tar.gz` into `build/bash/src/`;
2. replays the patches listed in `patches/` (currently one: the
   `!HAVE_NETWORK` fallback declaration fix);
3. configures with the freestanding clang and `-nostdlib`, so link probes
   resolve against `libc-fantuan` and host glibc can never leak in:
   `--host=x86_64-unknown-none --without-bash-malloc --disable-nls
   --disable-readline --enable-static-link`;
4. builds with `make`, strips and installs `build/bash/bash.elf` as
   `kernel/bash_program.bin` (embedded by `kernel/build.rs`, like dash; the
   binary is ~726 KiB and byte-reproducible).

`/bin/sh` resolves to bash when embedded (dash stays `/bin/dash` and the
`dash` console command); `tools/smoke-bash.sh` is the gate. The manifest still
carries `requires = ["posix-libc"]` because that requirement is now satisfied
by `libc-fantuan` (P1-P3); `appctl menu` keeps `CONFIG_APP_BASH` as the
app-layer availability marker.

## Isolation

Bash is a **separate executable**: it is never linked into the kernel, the
bootloader or any base library, no Rust crate depends on it, and the kernel
build does not read `apps/`. `tools/smoke-gpl.sh` proves this in CI (kernel
ELF has no bash symbols/paths, Cargo workspace has no edge into `apps/`, the
vendored tree is untouched by the build).
