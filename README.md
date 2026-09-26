# fantuan-apps - the application catalog

This branch is the catalog the `main` kernel consumes through
`tools/appctl`. One directory per app, each **vendored with its sources**:

```
apps/<name>/manifest.toml     metadata + build recipe (the contract)
apps/<name>/README.md         what it is, how to build/run (menu help)
apps/<name>/patches/          replayable portability patches
apps/<name>/src/              unmodified upstream sources
template/                     copy this to apps/<name>/ to start
```

`main` pins selected trees in `apps.lock` (source, revision, sha256) and adds
the `CONFIG_APP_<NAME>` menu entry. Ordinary apps are vendored into `main`
whole, so no fetch happens at build time; **bash is the exception** (below).
The integration contract and the branch model live in `docs/APPS.md` (main).

## The bash source bundle (why this branch carries a 11 MB tarball)

`main` deliberately tracks **no GPL source**: for GNU bash it keeps only the
metadata (`apps/bash/`: manifest, README, `COPYING`, the port patch, the
source pins) and builds the shell from a tarball obtained by
`tools/fetch-bash-src.sh`, which prefers the pinned upstream URL. This branch
is the **provisioned archive**: `apps/bash/src/bash-5.3.tar.gz` plus its
`.sig`, byte-identical to upstream and pinned by `apps/bash/src/SHA256SUMS`
(and by `manifest.toml:tarball_sha256` on both branches). It exists so that
the complete corresponding source for the shipped bash program is reachable
at a stable location even without ftp.gnu.org and so an air-gapped host can
build with `tools/fetch-bash-src.sh --from-branch`. Never edit those files.

## Pull requests

1. Fork/branch; copy `template/` to `apps/<name>/`.
2. Fill `manifest.toml`: `name`, `version`, SPDX `license`, `upstream`,
   `rev`, `abi_min`, `build` (`bmake` | `ninja` | `custom`), `description`,
   `gpl`. Add `deps` and `patches` only when real.
3. Keep upstream code byte-identical under `src/`; every portability change
   goes into `patches/NNNN-*.patch` and is listed in `manifest.patches`.
4. Run `tools/appctl/appctl.py verify --name <name>` locally; open the PR.
5. CI (`.github/workflows/ci.yml`) verifies manifests, hashes and licences,
   scans the SBOM and runs a per-app build smoke. PRs merge only green.

New apps land here first; `main` vendors them only when the owner picks them
(`tools/appctl/appctl.py add <name>`).

## Manifest rules

- `rev` is the upstream commit the `src/` tree came from; `upstream` is its
  repository. `abi_min` must not exceed `kernel-core`'s `ABI_VERSION`.
- `description` is one line and becomes the configuration menu prompt/help.
- No symlinks, no generated files, no binaries; sources only.

## Licensing

- SPDX in every manifest. BSD/MIT/Apache are preferred; anything copyleft
  must set `gpl = true`.
- `gpl = true` is refused in the kernel/base layer. It is accepted only in
  the apps layer, and only for names listed in `apps-catalog.toml`'s
  `[licensing] gpl_allow` (reviewed per app, registered in `THIRD_PARTY.md`).
- **bash is the registered exception**: GPLv3, the default `sh`, shipped as
  a separate program with complete corresponding sources (this branch's
  `apps/bash/src/` bundle, `COPYING` and the recipe on `main`) and never
  linked into the kernel or base libraries. Note that its built program *is*
  embedded in the kernel image as a blob, so the image counts as a
  distribution of bash; the in-image sources bundle at `/usr/src/bash/` and
  file-based delivery are the M14 deliverable (`docs/M14_LINUXUSERS.md`).
- This branch does not change the repository licence; see `LICENSE.md`.
