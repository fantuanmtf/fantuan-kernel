/* time.h shim for the freestanding mbedTLS build (ours, M11 R8).
 * MBEDTLS_PLATFORM_TIME_ALT replaces time() with the PIT-driven callback,
 * and MBEDTLS_PLATFORM_MS_TIME_ALT replaces mbedtls_ms_time(); this header
 * only supplies the time_t the mbedTLS platform headers reference.  The
 * struct timespec stays in the NetBSD headers the adapter already uses. */
#ifndef FANTUAN_MBEDTLS_TIME_H
#define FANTUAN_MBEDTLS_TIME_H

#ifdef __cplusplus
extern "C" {
#endif

typedef long time_t;

time_t time(time_t *);

#ifdef __cplusplus
}
#endif

#endif /* FANTUAN_MBEDTLS_TIME_H */
