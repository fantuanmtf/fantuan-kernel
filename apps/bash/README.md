# bash

**GNU Bash 5.3** — the default POSIX shell (M14-8). Separate GPLv3 program,
registered in [THIRD_PARTY.md](../../THIRD_PARTY.md) and allowed by
`apps-catalog.toml` `[licensing] gpl_allow = ["bash"]`; see the app contract in
[docs/APPS.md](../../docs/APPS.md).

## What ships

| Path | Content |
|---|---|
| `src/SHA256SUMS` | sha256 of the upstream tarball (`sha256sum -c` in `src/` once fetched) |
| `src/SOURCE` | release URL, retrieval date, signature and provenance notes |
| `COPYING` | GPLv3 text, extracted from the tarball |
| `patches/` | replayable fantuan-ABI patches (`0001-netopen-no-network-decls.patch`; replayed by `tools/build-bash.sh`) |
| `port/` | port record: `README.md` (config + build recipe) and `REQUIREMENTS.md` (the minimal POSIX surface) |

The pristine tarball (`bash-5.3.tar.gz`) and its `.sig` are **not tracked
here**: they are provisioned on the `fantuan-apps` branch under
`apps/bash/src/` and fetched at build time by `tools/fetch-bash-src.sh`
(cache → vendored tree → pinned upstream URL → branch).

## Source provision (GPLv3)

The complete corresponding source is the pristine upstream `bash-5.3.tar.gz`
(sha256 pin in `src/SHA256SUMS`, mirrored in `manifest.toml:tarball_sha256`
and `src/SOURCE`, GPG-verified against the GNU keyring) plus our patch under
`patches/`. It is deliberately **not** in `main`'s tree; `tools/fetch-bash-src.sh`
obtains and verifies it, and the `fantuan-apps` branch carries the archive
copy. Note that the kernel image embeds bash's built program as a blob, so
distributing the image is distributing bash — the in-image `/usr/src/bash/`
sources bundle is the M14 deliverable that closes this properly
(`docs/M14_LINUXUSERS.md`, `THIRD_PARTY.md`).

## How it is built (P3, 2026-09)

**bash builds and runs.** P1/P2 provided `libc-fantuan` and the process
layer; P3 filled the remaining libc surface (fnmatch/glob, regex, locale,
wchar, wordexp, popen, iconv/dl/dlfcn stubs) and `tools/build-bash.sh`
cross-builds the pristine tarball for `x86_64-unknown-none`:

1. `tools/fetch-bash-src.sh` resolves and verifies the tarball
   (`build/cache/bash-5.3.tar.gz`), which `build-bash.sh` then extracts into
   `build/bash/src/`; `tools/build.sh` runs both by default when
   `CONFIG_BASH=y`;
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
build does not read `apps/`. `tools/smoke-gpl.sh` proves this (kernel ELF has
no bash symbols/paths, Cargo workspace has no edge into `apps/`, the vendored
tree is untouched by the build).

Honesty note: because `kernel/build.rs` embeds the built program with
`include_bytes!`, the shipped kernel image *does* contain bash's bytes. The
firewall is about linking; the image is a distribution of bash and the source
provision above is what covers it. `tools/smoke-gpl.sh` asserts both halves —
no link, and the exact artifact hash present in the image — rather than only
the convenient one.
