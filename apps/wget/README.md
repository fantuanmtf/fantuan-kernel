# wget

**HTTP/HTTPS client** for the Live kernel's userspace. Catalog skeleton (C5):
the manifest is registered and gated on `requires = ["posix-libc"]`, no code
is built yet.

## Interim bridge (until M14-4)

The working implementation lives **inside the kernel** as the R7/R8 evidence:

- shell command: `wget [--insecure] http[s]://host[:port]/` in
  `kernel/src/shell/cmds_net.rs`, registered only when `CONFIG_TOOLS=y`;
- client: `wget` / `wget_https` in `kernel-net` (`CONFIG_NET=y`, TLS under
  `CONFIG_TLS=y`), sequenced by `kernel-net/src/c/rump_tools.c`;
- when `CONFIG_TOOLS=y` and `CONFIG_NET=n` the command is a one-line
  `not built (CONFIG_NET=n)` stub; the default minimal kernel links neither.

This bridge is deliberately non-default and interim: at **M14-4** the POSIX
layer makes `posix-libc` available, the catalog version is written/vendored
into `src/` (plus replayable `patches/`), and `CONFIG_APP_WGET=y` retires the
in-kernel command.

## What will ship

| Path | Content |
|---|---|
| `src/` | the tool's C sources (written at M14-4 from this skeleton) |
| `patches/` | replayable portability patches (empty until then) |
| `manifest.toml` | build recipe, ABI floor, licence |

Build policy: static link against the native ABI, never into the kernel.
