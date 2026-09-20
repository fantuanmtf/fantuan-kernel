/* string.h shim for the freestanding mbedTLS build (ours, M11 R8).
 * Declarations only: the definitions come from rump_shim_lib.c. */
#ifndef FANTUAN_MBEDTLS_STRING_H
#define FANTUAN_MBEDTLS_STRING_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

void *memcpy(void *, const void *, size_t);
void *memmove(void *, const void *, size_t);
void *memset(void *, int, size_t);
int memcmp(const void *, const void *, size_t);
size_t strlen(const char *);
int strcmp(const char *, const char *);
int strncmp(const char *, const char *, size_t);
char *strcpy(char *, const char *);
char *strncpy(char *, const char *, size_t);
char *strchr(const char *, int);
size_t strlcpy(char *, const char *, size_t);
size_t strlcat(char *, const char *, size_t);

#ifdef __cplusplus
}
#endif

#endif /* FANTUAN_MBEDTLS_STRING_H */
