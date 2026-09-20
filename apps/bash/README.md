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
| `patches/` | replayable musl/fantuan-ABI patches (empty until M14-8) |
| `port/` | early-start record: `README.md` (flags + blockers) and `REQUIREMENTS.md` (minimal POSIX surface) |

## Source provision (GPLv3)

GPLv3 compliance is satisfied with the complete corresponding source next to
the binary: the repo carries the pristine tarball under `apps/bash/src/`
(pinned by `apps.lock` and `SHA256SUMS`), and image assembly copies it into
the shipped image at `/usr/src/bash/` (sources bundle) together with
`COPYING` and `SOURCE`. The build recipe (manifest + this README + `patches/`)
travels with the same tree, so a recipient can rebuild the exact binary.

## How it is built (M14)

Nothing is built yet: libc/POSIX arrives with **M14-4** (musl) and M14-8 wires
the shell. The manifest therefore carries `requires = ["posix-libc"]`; until
that layer lands `tools/appctl menu` offers `CONFIG_APP_BASH` as unavailable
(`default n` plus a note) and never enables it. **C5 started the real port
work**: `port/README.md` records the exact configure/host flags attempted and
`port/REQUIREMENTS.md` the minimal libc/POSIX surface;
`tools/build-bash-spike.sh` reruns the cross-build attempt and records the
blocker list (configure failure, first missing headers and symbols) without
network access. bash does not run yet. At M14 the build script:

1. extracts `src/bash-5.3.tar.gz` into `build/apps/bash/`;
2. applies the replayable patches listed in `patches/` (musl build flags,
   job-control/pty and terminal defaults over the fantuan ABI);
3. configures against the in-system C library/native ABI
   (`--prefix=/usr --without-bash-malloc --disable-nls`) and installs
   `/usr/bin/bash`, with the image assembly choosing the `sh` link.

## Isolation

Bash is a **separate executable**: it is never linked into the kernel, the
bootloader or any base library, no Rust crate depends on it, and the kernel
build does not read `apps/`. `tools/smoke-gpl.sh` proves this in CI (kernel
ELF has no bash symbols/paths, Cargo workspace has no edge into `apps/`, the
vendored tree is untouched by the build).
