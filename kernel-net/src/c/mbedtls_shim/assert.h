/* assert.h shim for the freestanding mbedTLS build (ours, M11 R8).
 * mbedTLS's common.h includes <assert.h>; the kernel has no libc, so the
 * check routes to the adapter panic (release behavior is a trap, which is
 * the right thing for internal invariant violations). */
#ifndef FANTUAN_MBEDTLS_ASSERT_H
#define FANTUAN_MBEDTLS_ASSERT_H

#ifdef __cplusplus
extern "C" {
#endif

void fantuan_rump_panic(const void *, unsigned long)
    __attribute__((noreturn));

#ifdef NDEBUG
#define assert(expr) ((void)0)
#else
#define assert(expr) \
	((expr) ? (void)0 : fantuan_rump_panic("assert", (unsigned long)7))
#endif

#ifdef __cplusplus
}
#endif

#endif /* FANTUAN_MBEDTLS_ASSERT_H */
