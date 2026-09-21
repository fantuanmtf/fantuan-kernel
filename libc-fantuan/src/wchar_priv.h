/* libc-fantuan — internal UTF-8 helpers shared by wchar.c and
 * wchar_conv.c (P3). Not a public header. */
#ifndef WCHAR_PRIV_H
#define WCHAR_PRIV_H

#include <stddef.h>
#include <wchar.h>

int __fantuan_utf8_len(unsigned char c);
int __fantuan_utf8_decode(const unsigned char *s, size_t n, wchar_t *wc, size_t *used);

#endif /* WCHAR_PRIV_H */
