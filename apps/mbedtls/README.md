# mbedTLS 3.6.7

**Mbed TLS** - the TLS/HTTPS stack behind `CONFIG_TLS` (M11 R8). Registered
in [THIRD_PARTY.md](../../THIRD_PARTY.md); the app contract is in
[docs/APPS.md](../../docs/APPS.md). Dual-licensed upstream (Apache-2.0 OR
GPL-2.0-or-later); this project takes **Apache-2.0**, so the manifest carries
`gpl = false` and nothing GPL enters the kernel.

## What ships

| Path | Content |
|---|---|
| `src/mbedtls-3.6.7.tar.bz2` | pristine upstream release tarball |
| `src/SHA256SUMS` | upstream release checksums (`sha256sum -c` in `src/`) |
| `src/SOURCE` | release URL, retrieval date, hash and license notes |
| `LICENSE` | upstream LICENSE text (Apache-2.0 and GPL-2.0-or-later), extracted from the tarball |
| `patches/` | empty: the port lives in `kernel-net`, not in the sources |

## Build recipe (kernel integration)

`kernel-net/build.rs` reads the tarball only when `CONFIG_TLS=y` (and only for
`x86_64-unknown-none`), extracts `include/` and `library/` into cargo's
`OUT_DIR`, and compiles the subset below with clang, freestanding, exactly
like the imported rump slice (`-nostdinc`, `-mno-sse`, `-mcmodel=large`,
`-D_KERNEL`). The adapter is compiled with `-Wall -Wextra`; the extracted
upstream files keep their warnings like every other vendored source.

- config: `kernel-net/src/c/mbedtls_config.h` via `-DMBEDTLS_CONFIG_FILE`;
- libc shims: `kernel-net/src/c/mbedtls_shim/` (`string.h`, `stdlib.h`,
  `stdio.h`, `time.h`, `assert.h`) - the freestanding declarations mbedTLS
  includes; the definitions already come from the rump adapter
  (`rump_shim_lib.c`, `rump_shim_printf.c`);
- glue: `rump_tls_entropy.c` (allocators, PIT time, CSPRNG, platform
  callbacks), `rump_tls.c` (socket BIO + client state machine),
  `rump_tls_kat.c` (boot self-tests).

Compiled subset (TLS 1.2 client + ECDHE-RSA + AES-GCM + SHA-256 + X.509/RSA):
`aes`, `asn1parse`, `bignum`, `bignum_core`, `constant_time`, `ctr_drbg`,
`ecdh`, `ecp`, `ecp_curves`, `entropy`, `error`, `gcm`, `md`, `oid`, `pk`,
`pk_wrap`, `pkparse`, `platform`, `platform_util`, `rsa`, `sha256`,
`ssl_ciphersuites`, `ssl_client`, `ssl_msg`, `ssl_tls`, `ssl_tls12_client`,
`x509`, `x509_crt`.

## Kernel configuration choices

| mbedTLS feature | Kernel choice |
|---|---|
| filesystem (`MBEDTLS_FS_IO`) | off - certificates arrive as compiled-in DER, not files |
| threading (`MBEDTLS_THREADING_C`) | off - single net task, no locks |
| timing (`MBEDTLS_TIMING_C`) | off - certs and handshakes are tick-bounded by the caller |
| `MBEDTLS_NET_C` | off - the transport is the rump socket layer |
| entropy | `MBEDTLS_NO_PLATFORM_ENTROPY` + `MBEDTLS_ENTROPY_HARDWARE_ALT`: `mbedtls_hardware_poll` returns bytes from a SHA-256 counter CSPRNG seeded at boot |
| time | `MBEDTLS_PLATFORM_TIME_ALT`: `mbedtls_time()` returns PIT ticks / 100. `MBEDTLS_HAVE_TIME_DATE` is deliberately **off** (no RTC and no trusted wall clock), so certificate notBefore/notAfter dates are not checked - the pinned-CA trust boundary is the claim |
| memory | `MBEDTLS_PLATFORM_MEMORY`: `mbedtls_platform_set_calloc_free` routes to the adapter `kmem` arena (frame-backed; freed bump items are not yet recycled, a documented R2 limit) |
| TLS versions | TLS 1.2 client only (`MBEDTLS_SSL_PROTO_TLS1_2`, no TLS 1.3) |
| cipher suites | `ECDHE-RSA-AES128-GCM-SHA256` (secp256r1), via `mbedtls_ssl_conf_ciphersuites` |
| X.509 | parse/verify from DER, RSA + PKCS#1 v1.5/PSS signatures, SHA-256 |
| PEM/base64 | off - the pinned CA is embedded as DER |
| self-tests | off in the library; the project runs its own KATs at boot |

## Entropy

There is no `getrandom(2)` in the kernel. `rump_tls_entropy.c` seeds a small
SHA-256 counter CSPRNG at boot: 32 words from RDRAND when the CPU exposes it
(CPUID.01H:ECX[30]), mixed with RDTSC and the PIT tick for the fallback path.
`mbedtls_hardware_poll()` draws from that CSPRNG; mbedTLS entropy conditioning
and `CTR_DRBG` run on top. On a deterministic VM without RDRAND the seed is
weak by construction - documented, not a security claim; there is no IPsec or
long-term key material on this path.

## The pinned CA

`build/smoke-net-tls/ca.der` (generated per smoke run) is embedded into the
kernel by `build.rs` when present; HTTPS verification is on by default and a
`wget --insecure` flag (used only by the optional external phase) prints an
explicit warning. The offline TLS fixture in `tools/smoke-net.sh` signs its
server certificate with that per-run CA and serves `test.fantuan:18443`
(reachable from the guest as `10.0.2.2`).

## App path (M14)

`tools/appctl` treats this as a pinned upstream tarball
(`source = "upstream"`): `sync`/`upgrade` leave it alone. The manifest requires
`kernel-net` (already implemented; the kernel build consumes it under
`CONFIG_TLS`) and `posix-libc` (M14). While `AVAILABLE_REQUIRES` is empty the
menu lists both as unmet, so `CONFIG_APP_MBEDTLS` stays `default n`. The
kernel integration does not go through that symbol: it is gated by
`CONFIG_TLS`.
