/* inttypes.h shim for the freestanding mbedTLS build (ours, M11 R8).
 * clang's <inttypes.h> include_next's the libc one, which does not exist on
 * x86_64-unknown-none; mbedTLS only needs the 64-bit format macros.  The
 * guards keep the adapter build (which also sees the NetBSD format headers)
 * warning-free. */
#ifndef FANTUAN_MBEDTLS_INTTYPES_H
#define FANTUAN_MBEDTLS_INTTYPES_H

#include <stdint.h>

#ifndef PRId64
#define PRId64 "lld"
#define PRIi64 "lli"
#define PRIu64 "llu"
#define PRIx64 "llx"
#define PRIX64 "llX"
#endif

#endif /* FANTUAN_MBEDTLS_INTTYPES_H */
