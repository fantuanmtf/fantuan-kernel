/* stdlib.h shim for the freestanding mbedTLS build (ours, M11 R8).
 * calloc/free are routed through mbedtls_platform_set_calloc_free() to the
 * adapter kmem arena; the prototypes here only keep compilers quiet. */
#ifndef FANTUAN_MBEDTLS_STDLIB_H
#define FANTUAN_MBEDTLS_STDLIB_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

void *calloc(size_t, size_t);
void free(void *);
void abort(void) __attribute__((noreturn));

#ifdef __cplusplus
}
#endif

#endif /* FANTUAN_MBEDTLS_STDLIB_H */
