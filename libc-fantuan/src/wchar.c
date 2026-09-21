/* libc-fantuan — wide-character conversions (P3).
 *
 * UTF-8 based, stateless across partial sequences: an incomplete sequence
 * reports (size_t)-2 and the caller retries with more input (the POSIX
 * mbstate contract). The C locale keeps MB_CUR_MAX == 1, so bash's
 * single-byte paths remain in charge at runtime; wstring.c carries the
 * wcs* helpers. */
#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

#include "wchar_priv.h"

int __fantuan_utf8_len(unsigned char c)
{
    if (c < 0x80) return 1;
    if ((c & 0xE0) == 0xC0) return 2;
    if ((c & 0xF0) == 0xE0) return 3;
    if ((c & 0xF8) == 0xF0) return 4;
    return 0;
}

int __fantuan_utf8_decode(const unsigned char *s, size_t n, wchar_t *wc, size_t *used)
{
    int len = __fantuan_utf8_len(s[0]);
    unsigned int cp;
    int i;
    if (len == 0) {
        errno = EILSEQ;
        return -1;
    }
    if (n < (size_t)len) {
        return -2;
    }
    if (len == 1) {
        *wc = s[0];
        *used = 1;
        return 1;
    }
    cp = (unsigned int)(s[0] & (0xFF >> (len + 1)));
    for (i = 1; i < len; i++) {
        if ((s[i] & 0xC0) != 0x80) {
            errno = EILSEQ;
            return -1;
        }
        cp = (cp << 6) | (unsigned int)(s[i] & 0x3F);
    }
    if ((len == 2 && cp < 0x80) || (len == 3 && cp < 0x800) ||
        (len == 4 && (cp < 0x10000 || cp > 0x10FFFF))) {
        errno = EILSEQ;
        return -1;
    }
    *wc = (wchar_t)cp;
    *used = (size_t)len;
    return 1;
}

size_t mbrtowc(wchar_t *pwc, const char *s, size_t n, mbstate_t *ps)
{
    wchar_t wc;
    size_t used = 0;
    int r;
    if (ps != NULL) {
        ps->__count = 0;
    }
    if (s == NULL) {
        return 0;
    }
    if (n == 0) {
        return (size_t)-2;
    }
    r = __fantuan_utf8_decode((const unsigned char *)s, n, &wc, &used);
    if (r < 0) {
        return r == -2 ? (size_t)-2 : (size_t)-1;
    }
    if (wc == 0) {
        return 0;
    }
    if (pwc != NULL) {
        *pwc = wc;
    }
    return used;
}

size_t wcrtomb(char *s, wchar_t wc, mbstate_t *ps)
{
    unsigned int cp = (unsigned int)wc;
    if (ps != NULL) {
        ps->__count = 0;
    }
    if (s == NULL) {
        return 1;
    }
    if (cp < 0x80) {
        s[0] = (char)cp;
        return 1;
    }
    if (cp < 0x800) {
        s[0] = (char)(0xC0 | (cp >> 6));
        s[1] = (char)(0x80 | (cp & 0x3F));
        return 2;
    }
    if (cp >= 0xD800 && cp <= 0xDFFF) {
        errno = EILSEQ;
        return (size_t)-1;
    }
    if (cp < 0x10000) {
        s[0] = (char)(0xE0 | (cp >> 12));
        s[1] = (char)(0x80 | ((cp >> 6) & 0x3F));
        s[2] = (char)(0x80 | (cp & 0x3F));
        return 3;
    }
    if (cp <= 0x10FFFF) {
        s[0] = (char)(0xF0 | (cp >> 18));
        s[1] = (char)(0x80 | ((cp >> 12) & 0x3F));
        s[2] = (char)(0x80 | ((cp >> 6) & 0x3F));
        s[3] = (char)(0x80 | (cp & 0x3F));
        return 4;
    }
    errno = EILSEQ;
    return (size_t)-1;
}

size_t mbrlen(const char *s, size_t n, mbstate_t *ps)
{
    return mbrtowc(NULL, s, n, ps);
}

int mbsinit(const mbstate_t *ps)
{
    return ps == NULL || ps->__count == 0;
}

wint_t btowc(int c)
{
    if (c == -1) {
        return WEOF;
    }
    return (wint_t)(unsigned char)c;
}

int wctob(wint_t c)
{
    if (c < 0x80) {
        return (int)c;
    }
    return -1;
}

int mblen(const char *s, size_t n)
{
    wchar_t wc;
    size_t used = 0;
    int r;
    if (s == NULL) {
        return 0;
    }
    if (*s == '\0') {
        return 0;
    }
    r = __fantuan_utf8_decode((const unsigned char *)s, n, &wc, &used);
    if (r < 0) {
        return -1;
    }
    (void)used;
    return r;
}

int mbtowc(wchar_t *pwc, const char *s, size_t n)
{
    wchar_t wc;
    size_t used = 0;
    int r;
    if (s == NULL) {
        return 0;
    }
    if (*s == '\0') {
        return 0;
    }
    r = __fantuan_utf8_decode((const unsigned char *)s, n, &wc, &used);
    if (r < 0) {
        return -1;
    }
    if (pwc != NULL) {
        *pwc = wc;
    }
    return r;
}

int wctomb(char *s, wchar_t wc)
{
    size_t n;
    if (s == NULL) {
        return 0;
    }
    n = wcrtomb(s, wc, NULL);
    return n == (size_t)-1 ? -1 : (int)n;
}
