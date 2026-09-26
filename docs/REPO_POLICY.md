# Repository policy

The governance rules for this repository: where code lives, which mirror may
hold what, how the licence layers are kept apart, and who publishes. These are
rules of record, not suggestions — the gates in `tools/smoke-gpl.sh`,
`tools/smoke-config.sh` and `tools/smoke-apps.sh` assert most of them.

## 1. Remotes and their roles

| Remote | Role | Branches |
|---|---|---|
| `origin` (GitHub) | the primary repository; development, issues, pull requests | all (`main`, `fantuan-apps`, `package`) |
| `gitlab` | a full mirror of GitHub, kept identical | all |
| `codeberg` | the **pure-kernel mirror** | `main` only — never a catalog or tooling branch |

- `codeberg` exists so that "the kernel" can be cloned from a smaller,
  non-App-catalog source. It must never receive `fantuan-apps` or `package`,
  and it therefore holds **no copy of the GPL source bundle** (section 4):
  a Codeberg-only clone must still be able to build, which is why the bash
  tarball is fetched from its pinned upstream URL rather than from a branch.
- Branches rebuilt from `main` (see `tools/mkbranches.sh`) are published with
  `git push --force-with-lease=<branch>:<expected-sha>`, and only to `origin`
  and `gitlab`.
- **GitLab protects `main` against force pushes** (GitHub does not). A history
  rewrite therefore publishes to GitHub and Codeberg immediately and leaves
  GitLab behind until the owner either allows force pushes on that protected
  branch (Settings > Repository > Protected branches) or pushes it by hand.
  Check with `git ls-remote --heads gitlab` afterwards — a mirror that is
  silently one rewrite behind is worse than a red gate.
- Tags: the repository does not publish tags. A release tag prepared for the
  owner stays local (`docs/HANDOVER.md` §2).

## 2. Branches

| Branch | Holds | Licence layer |
|---|---|---|
| `main` | the kernel: `boot`, `boot-bios`, `kernel*`, `kernel-core`, `abi`, `user`, `libc-fantuan`, `drivers`, `tools`, `config`, `docs` | BSD-3-Clause (the project's own code) |
| `fantuan-apps` | the application catalog: one directory per app with sources, patches, manifest, README — **plus the GPL source bundle** at `apps/bash/src/` | per app (SPDX in each manifest) |
| `package` | the packaging tooling `tools/appctl/` (developed here, vendored back to `main`) | BSD-3-Clause |

`main` is the integration point: applications enter it by vendoring
(`tools/appctl/appctl.py add <name>`), pinned by `apps.lock`. The branch model
in full is `docs/APPS.md`.

## 3. What the kernel layer may carry

- **No copyleft source code.** `main`'s tree contains no GPL/LGPL source and no
  GPL archive; the history has been rewritten once to remove the bash tarball
  that C3 had vendored, so the rule holds for the whole history, not just the
  tip. The check is `tools/smoke-gpl.sh` phase `[a1]`.
- **A copyleft *binary* may only be a separate program.** Nothing copyleft is
  ever linked into the kernel, the bootloader or the base libraries: no Cargo
  edge into `apps/`, no shell symbols in the kernel ELF, no app paths in it
  (`tools/smoke-gpl.sh` phase `[e]`, `tools/smoke-apps.sh`).
- **Documented exceptions live in `THIRD_PARTY.md`.** Every bundled component
  is registered there with version, origin, licence, usage and modifications;
  importing or bundling anything without a register entry is a policy
  violation. Imported upstream sources keep their headers and are exempt from
  the 300-line rule; our wrappers are not.
- **GNU bash (GPL-3.0-or-later)** is the one registered copyleft program in
  the default image: it is the default `sh` **and the login shell** (DESIGN
  §10), it is never linked, and its built program is embedded in the kernel
  image as an opaque stripped blob — a deliberate, recorded exception whose
  source provision is section 4. The M14 task is to move it to file-based
  delivery (`docs/M14_LINUXUSERS.md`). Sharing an address space with the
  kernel is not "linking" in the firewall's sense, but it is distribution:
  treat every shipped image as a bash distribution and keep section 4 true.

## 4. Source provision for the embedded shell

- The complete corresponding source is the pristine upstream
  `bash-5.3.tar.gz` (sha256
  `0d5cd86965f869a26cf64f4b71be7b96f90a3ba8b3d74e27e8e9d9d5550f31ba`,
  GPG-verified against the GNU keyring) plus our patch in
  `apps/bash/patches/`.
- It is **not tracked on `main`** (section 3). `main` keeps the pins
  (`apps/bash/src/SOURCE`, `apps/bash/src/SHA256SUMS`,
  `apps/bash/manifest.toml:tarball_sha256`), the patch, `COPYING` and the
  record in `manifest.toml:source_provision`.
- `tools/fetch-bash-src.sh` resolves and verifies the tarball from, in order:
  `$FANTUAN_BASH_TARBALL`, `build/cache/`, a vendored `apps/bash/src/`, the
  pinned upstream URL, or `--from-branch` (the `fantuan-apps` mirror).
  `FANTUAN_OFFLINE=1` forbids the network; `CONFIG_BASH=n` /
  `FANTUAN_BUILD_BASH=0` build without a shell.
- The `fantuan-apps` branch carries the archive copy so the source stays
  reachable at a stable location, and `tools/smoke-gpl.sh` phase `[a4]` checks
  that the mirror's bytes match the pin.

## 5. Publication

- Commits land on `main` (or are proposed by pull request against it).
- **The owner pushes.** A working batch stops with a clean tree, a local
  commit and its verification evidence; publishing is a separate, explicit
  step (`docs/HANDOVER.md` §8.7).
- The mirror roles in section 1 are the whole story: no fourth remote, no
  branch on Codeberg, no published tags.
