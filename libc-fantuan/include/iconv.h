/* libc-fantuan — iconv.h (P3): linkable but unsupported. All conversions
 * report EINVAL/-(size_t)1; callers (bash's fnxform fallback) degrade to
 * the identity transform, which is correct for the C locale. */
#ifndef _ICONV_H
#define _ICONV_H

#include <stddef.h>

typedef void *iconv_t;

iconv_t iconv_open(const char *tocode, const char *fromcode);
size_t iconv(iconv_t cd, char **inbuf, size_t *inbytesleft,
             char **outbuf, size_t *outbytesleft);
int iconv_close(iconv_t cd);

#endif /* _ICONV_H */
