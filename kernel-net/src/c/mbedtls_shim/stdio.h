/* stdio.h shim for the freestanding mbedTLS build (ours, M11 R8).
 * Only snprintf/vsnprintf are used (X.509 name formatting and error paths);
 * their definitions come from rump_shim_printf.c. */
#ifndef FANTUAN_MBEDTLS_STDIO_H
#define FANTUAN_MBEDTLS_STDIO_H

#include <stddef.h>
#include <stdarg.h>

#ifdef __cplusplus
extern "C" {
#endif

int snprintf(char *, size_t, const char *, ...);
int vsnprintf(char *, size_t, const char *, va_list);

#ifdef __cplusplus
}
#endif

#endif /* FANTUAN_MBEDTLS_STDIO_H */
