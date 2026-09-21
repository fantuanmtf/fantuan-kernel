# bash portability patches

P3 replays these on top of the pristine `src/bash-5.3.tar.gz` at build time
(`tools/build-bash.sh`); every file here is listed in the manifest `patches`
array so `appctl upgrade` restores it.

| Patch | Why |
|---|---|
| `0001-netopen-no-network-decls.patch` | `lib/sh/netopen.c`'s `!HAVE_NETWORK` fallback calls `internal_error()`/`_()` that upstream only declares inside the `HAVE_NETWORK` branch; without netinet/in.h the fallback compiles and fails. Two include lines, no behaviour change. |

Rules:

- keep `src/bash-5.3.tar.gz` pristine; patches are the only source changes;
- no patch may pull bash code into the kernel or base libraries; bash stays a
  separate executable linked only against libc-fantuan (GPL firewall).
