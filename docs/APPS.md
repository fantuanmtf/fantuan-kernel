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

## Budgets

- Boot + kernel + shell stay within **300 MiB** (hard build check); the
  app layer is unbounded, and the kernel is never rebuilt because an app
  was added.
