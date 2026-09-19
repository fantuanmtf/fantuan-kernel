# Vendored applications

`apps/<name>/` holds one selected application **with its sources**, pinned in
`apps.lock`. The tree is populated by `tools/appctl` from the catalog
(`apps-catalog.toml`, whose default source is the `fantuan-apps` branch of this
repository) and is committed to `main`, so the kernel builds offline and
reproducibly. The owner decides the app list; `bash/` (C3) is the first real
app and pins an upstream release tarball instead of a catalog revision.

```
apps/<name>/manifest.toml     metadata and the build recipe (see docs/APPS.md)
apps/<name>/README.md         what it is, how to build/run (also the menu help)
apps/<name>/patches/          replayable portability patches
apps/<name>/src/              vendored upstream sources (unmodified base)
```

Build output goes to `build/apps/<name>/` (gitignored); the kernel is never
rebuilt because an app was added.

## appctl

`tools/appctl/appctl.py` is the only writer of this directory and of
`apps.lock`. Python stdlib only; run it from the repository root.

```sh
tools/appctl/appctl.py list                         # catalog + vendored + CONFIG_APP_* state
tools/appctl/appctl.py add <name>                   # vendor from the default catalog source
tools/appctl/appctl.py add <name> --from <branch|path>
tools/appctl/appctl.py sync [--from <branch|path>] [--name <name>]
tools/appctl/appctl.py upgrade [--name <name>]      # newer revisions, local patches kept
tools/appctl/appctl.py verify [--name <name>] [--apps-layer]
tools/appctl/appctl.py menu                         # write config/apps/<name>.kconfig
tools/appctl/appctl.py sbom [-o sbom.json]
```

`sync` copies only `{manifest.toml,README.md,patches/,src/}` and records the
source name, the resolved revision (`git rev-parse` for git sources, fallback
`unversioned`) and the tree hash in `apps.lock`. `upgrade` re-syncs newer
revisions and restores every local patch listed in the manifest.

## apps.lock

```toml
version = 1
[[app]]
name = "example"
version = "1.0"
license = "BSD-2-Clause"
gpl = false
source = "fantuan-apps"      # catalog source, "path:<dir>", or "upstream"
rev = "<git commit>"
sha256 = "<tree hash>"
```

`source = "upstream"` marks an app pinned from an upstream release tarball
(bash): `sync`/`upgrade` report it and leave it alone, since it has no
catalog revision to re-sync. `requires` in the manifest (optional) names
layers the app needs; while a layer is unavailable, `menu` writes the
`CONFIG_APP_<NAME>` fragment with `default n` and an unavailable note
(`app_manifest.AVAILABLE_REQUIRES`; `posix-libc` opens at M14).

Tree hash: sha256 over the sorted lines `"<relpath> <file-sha256>\n"` for every
regular file below `apps/<name>/` (bytewise-sorted relpaths, forward slashes).
`verify` recomputes it and fails on any drift. Symlinks are refused.

## Licensing and the GPL firewall

- Every manifest carries an SPDX `license` and a `gpl` flag.
- `verify` refuses any `gpl = true` app in the kernel/base layer. It is only
  accepted with `--apps-layer` **and** an entry in the catalog's
  `[licensing] gpl_allow` list; `menu` skips such apps otherwise. The kernel
  and base libraries stay BSD/MIT/Apache-only.
- bash is the registered GPLv3 exception (`docs/APPS.md`): it is vendored in
  `apps/bash/` (C3) with its complete sources, `COPYING` and a manifest in
  the allow list, and it is never linked into the kernel or base libraries.

## Adding an app

1. Land the app in the `fantuan-apps` branch (copy `template/`, fill the
   manifest, keep sources unmodified under `src/`, put ports in `patches/`).
2. `tools/appctl/appctl.py add <name>` here; commit `apps/<name>`,
   `apps.lock` and the generated `config/apps/<name>.kconfig`.
3. Enable it with `tools/kconfig.py --symbol APP_<NAME>=Y`.
