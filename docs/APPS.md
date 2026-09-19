# Applications, packages and the branch model

Direction: the kernel stays a kernel. Tools live outside it and enter the
build only through configuration; nothing optional is linked in by default.

## Branch model (current)

| Branch | Role |
|---|---|
| `main` | the kernel: boot + kernel + shell (+ apps vendored under `apps/` when configured) |
| `fantuan-apps` | the applications catalog: one directory per app with its sources, patches, manifest and README; community PRs land here |
| `package` | the package tooling (`tools/appctl`) and the catalog/build glue |

These are branches of this repository for now. If the catalog outgrows the
kernel repo it splits into standalone repositories with `git subtree
split` (history follows); the integration contract below stays identical.

Mirrors: GitHub carries every branch (`main`, `fantuan-apps`, `package`);
Codeberg mirrors the **pure kernel only** (`main`), with no catalog or
tooling branches.

## Integration point on main

```
apps/<name>/manifest.toml     app metadata and build recipe
apps/<name>/README.md         what it is, how to build/run (menu help)
apps/<name>/patches/          portability patches (replayable)
apps/<name>/src/              vendored upstream sources (unmodified base)
apps-catalog.toml             catalog sources (fantuan-apps / overlay / local)
apps.lock                     source branch/commit + sha256 per vendored tree
build/apps/<name>/            scratch build dir (gitignored)
```

Vendoring is the agreed policy: selected tools are copied **with their
sources** into `apps/<name>/` and pinned in `apps.lock`, so `main` builds
offline and reproducibly. `tools/appctl` (developed on the `package`
branch) syncs, upgrades, verifies (hash/licence/ABI), feeds the
`CONFIG_APP_*` menu entries and emits the SBOM; `main` only consumes the
generated Kconfig fragments.

## Manifest (every app has one)

```toml
name = "example"
version = "1.0"
license = "BSD-2-Clause"        # SPDX
upstream = "https://...git"
rev = "<commit>"
abi_min = 1                     # <= fantuan_abi::ABI_VERSION at build time
build = "bmake"                 # bmake | ninja | custom
deps = []
patches = ["patches/0001-port.patch"]
requires = []                   # optional layers; while unmet the menu keeps it off
description = "one line; shown as the menu prompt/help"
gpl = false                     # true only in the apps layer, never linked
```

## Licensing

- SPDX in every manifest; CI refuses any GPL manifest in the kernel/base
  layer and records the app-layer ones in the SBOM.
- **bash is the registered exception**: it is the default `sh`, GPLv3,
  shipped as a separate program with complete corresponding sources
  (vendored `src/`, `COPYING`, plus a sources copy in the image at
  `/usr/src/bash`), and it is never linked into the kernel or base
  libraries. `THIRD_PARTY.md` keeps the human-readable register.
- Desktop (XFCE, CDE) is vendored in the `fantuan-apps` branch and synced
  only for desktop profiles; the kernel repo stays light by default.

## ABI and incrementality

- `abi_min` is checked against `fantuan_abi::ABI_VERSION` before build.
- Manifests are content-hashed and each app builds in its own directory:
  adding or removing one app rebuilds only that app plus image assembly.

## Implemented in C2 (2026-09)

`main` carries the integration side: `apps/README.md`, `apps-catalog.toml`
(default source = the `fantuan-apps` branch of this repo, optional disabled
`overlay` local path, `[licensing] gpl_allow` = the apps-layer list) and an
empty `apps.lock`. `tools/appctl/` is the same stdlib client that lives on the
`package` branch:

```sh
tools/appctl/appctl.py list
tools/appctl/appctl.py add <name> [--from <branch|path>]
tools/appctl/appctl.py sync [--from <branch|path>] [--name <name>]
tools/appctl/appctl.py upgrade [--name <name>]
tools/appctl/appctl.py verify [--name <name>] [--apps-layer]
tools/appctl/appctl.py menu
tools/appctl/appctl.py sbom [-o sbom.json]
```

- `add`/`sync` copy `{manifest.toml,README.md,patches/,src/}` and pin the
  source name, the resolved revision (`git rev-parse` for git sources) and the
  tree sha256 in `apps.lock`. Schema: `version = 1` plus one `[[app]]` table
  per app (`name`, `version`, `license`, `gpl`, `source`, `rev`, `sha256`);
  the tree hash is sha256 over the sorted `"<relpath> <file-sha256>"` lines.
- `upgrade` re-syncs newer revisions and restores every local patch listed in
  the manifest.
- `verify` recomputes the hash, requires name/version/SPDX license/
  description/`abi_min`/build, checks `abi_min <= fantuan_abi::ABI_VERSION`
  and enforces the **GPL firewall**: a `gpl = true` manifest is refused in the
  kernel/base layer and accepted only with `--apps-layer` **and** an entry in
  `[licensing] gpl_allow`. Failures exit 1.
- `menu` writes one `config/apps/<name>.kconfig` per non-refused app with the
  `CONFIG_APP_<NAME>` bool and the manifest description as prompt/help;
  `tools/kconfig.py` merges every fragment. GPL apps outside the allow list
  are skipped.
- `sbom` prints JSON (`name`, `version`, `license`, `gpl`, `source`, `rev`,
  `sha256`).
- `tools/smoke-apps.sh` proves the pipeline offline on a fixture catalog
  (add/sync/lock hash, kernel-layer GPL refusal, apps-layer allow list, menu +
  `kconfig.py --check`, sbom, remove, corruption).

Branch skeletons live (gitignored) in `build/branch-skeletons/`; the
owner-run `tools/mkbranches.sh` creates/updates the local `fantuan-apps` and
`package` branches from `main` and prints the `git push -u origin ...`
commands. It never runs automatically. No real app was vendored in C2; bash
lands in C3.

## Implemented in C3 (2026-09)

`apps/bash/` vendors GNU Bash 5.3 as the first real app (the GPLv3 default
shell): `manifest.toml` (`license = "GPL-3.0-or-later"`, `gpl = true`,
`upstream` = the release tarball URL, `rev = "unversioned"`,
`tarball_sha256`, `build = "custom"`, `abi_min = 1`), the pristine
`src/bash-5.3.tar.gz` with its `.sig`, `SHA256SUMS` and `SOURCE`, the GPLv3
text in `COPYING`, and a `patches/` directory that stays empty until M14-8.
`apps.lock` pins the whole tree (`source = "upstream"` - a pinned release
tarball, which `sync`/`upgrade` report and leave alone);
`apps-catalog.toml` lists bash in `[licensing] gpl_allow`; `THIRD_PARTY.md`
carries the register row and the source-provision note.

The manifest gains the optional `requires` field (backward compatible: old
tooling ignores it). `posix-libc` is not in `app_manifest.AVAILABLE_REQUIRES`
until M14, so `menu` offers bash as unavailable: the generated fragment keeps
`default n` and carries the "(unavailable: requires posix-libc (M14
POSIX/libc layer))" prompt plus the stderr note
`menu: bash: unavailable (requires ...)`; `kconfig.py` cannot pick
`CONFIG_APP_BASH=y` up from defaults. `verify` is unaffected by `requires`
(integrity/licence only): bash still fails in the kernel/base layer and
passes with `--apps-layer` because it is in `gpl_allow`.

Source-provision policy: the repo ships the complete corresponding source
(the pristine tarball, pinned by `apps.lock` + `SHA256SUMS`); image assembly
adds it to `/usr/src/bash/` with `COPYING` and `SOURCE`, so the built binary
travels with its exact sources and build recipe. bash is a separate M14-8
executable over the native ABI, never linked into the kernel, bootloader or
base libraries.

`tools/smoke-gpl.sh` (offline) proves the five invariants: tarball sha256 and
`COPYING` provenance; kernel/base refusal and apps-layer allow; the `requires`
gate in `menu`; the SBOM entry (`gpl = true`, `GPL-3.0-or-later`); and
kernel/base isolation (the x86_64 kernel rebuild has no Cargo edge into
`apps/`, leaves the bash tree hash and mtimes untouched, and the kernel ELF
contains no bash symbols or app paths).

## Budgets

- Boot + kernel + shell stay within **300 MiB** (hard build check); the
  app layer is unbounded, and the kernel is never rebuilt because an app
  was added.
