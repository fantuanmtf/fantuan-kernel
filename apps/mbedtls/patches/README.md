# mbedTLS portability patches

Empty by design: the kernel build takes the pristine upstream sources and
compiles a subset directly (no source patch). The port lives outside the
tarball, in `kernel-net`:

- `src/c/mbedtls_config.h` - the kernel configuration (no `MBEDTLS_FS_IO`,
  no `MBEDTLS_THREADING_C`, no `MBEDTLS_NET_C`, TLS 1.2 client only);
- `src/c/mbedtls_shim/` - the tiny freestanding libc headers mbedTLS expects
  (`string.h`, `stdlib.h`, `stdio.h`, `time.h`, `assert.h`);
- `src/c/rump_tls_entropy.c` / `rump_tls.c` / `rump_tls_kat.c` - the platform
  glue (allocators, PIT time, boot-seeded CSPRNG, socket BIO, KATs).

If a future mbedTLS release needs a real patch, it goes here, is listed in
`manifest.toml` `patches`, and is replayed at build time against the
pristine tarball.

`.gitkeep` keeps this directory present while it is empty.
