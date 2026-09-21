/* libc-fantuan — restartable wide-character conversions (P3).
 * Split from wchar.c to stay inside the repo' 300-line convention. */
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

#include "wchar_priv.h"

size_t mbstowcs(wchar_t *dest, const char *src, size_t n)
{
    const char *s = src;
    return mbsrtowcs(dest, &s, n, NULL);
}

size_t wcstombs(char *dest, const wchar_t *src, size_t n)
{
    const wchar_t *s = src;
    return wcsrtombs(dest, &s, n, NULL);
}

size_t mbsrtowcs(wchar_t *dst, const char **src, size_t len, mbstate_t *ps)
{
    const char *s = src != NULL ? *src : NULL;
    size_t count = 0;
    if (ps != NULL) {
        ps->__count = 0;
    }
    if (s == NULL) {
        return 0;
    }
    while (*s != '\0') {
        wchar_t wc;
        size_t used = 0;
        int r;
        if (len > 0 && count >= len) {
            return count;
        }
        r = __fantuan_utf8_decode((const unsigned char *)s, strlen(s), &wc, &used);
        if (r < 0) {
            return (size_t)-1;
        }
        if (dst != NULL) {
            dst[count] = wc;
        }
        count++;
        s += used;
    }
    if (dst != NULL) {
        dst[count] = 0;
        *src = NULL;
    }
    return count;
}

size_t wcsrtombs(char *dst, const wchar_t **src, size_t len, mbstate_t *ps)
{
    const wchar_t *s = src != NULL ? *src : NULL;
    size_t count = 0;
    if (ps != NULL) {
        ps->__count = 0;
    }
    if (s == NULL) {
        return 0;
    }
    while (*s != 0) {
        char tmp[4];
        size_t n = wcrtomb(tmp, *s, NULL);
        if (n == (size_t)-1) {
            return (size_t)-1;
        }
        if (len > 0 && count + n > len) {
            return count;
        }
        if (dst != NULL) {
            memcpy(dst + count, tmp, n);
        }
        count += n;
        s++;
    }
    if (dst != NULL) {
        dst[count] = '\0';
        *src = NULL;
    }
    return count;
}

size_t mbsnrtowcs(wchar_t *dst, const char **src, size_t nms, size_t len, mbstate_t *ps)
{
    const char *s = src != NULL ? *src : NULL;
    size_t count = 0;
    if (s == NULL) {
        return 0;
    }
    while (nms > 0 && *s != '\0') {
        wchar_t wc;
        size_t r = mbrtowc(&wc, s, nms, ps);
        if (r == (size_t)-1) {
            return (size_t)-1;
        }
        if (r == (size_t)-2) {
            break;
        }
        if (r == 0) {
            break;
        }
        if (len > 0 && count >= len) {
            return count;
        }
        if (dst != NULL) {
            dst[count] = wc;
        }
        count++;
        s += r;
        nms -= r;
    }
    if (dst != NULL) {
        dst[count] = 0;
        *src = NULL;
    }
    return count;
}

size_t wcsnrtombs(char *dst, const wchar_t **src, size_t nwc, size_t len, mbstate_t *ps)
{
    const wchar_t *s = src != NULL ? *src : NULL;
    size_t count = 0;
    if (s == NULL) {
        return 0;
    }
    while (nwc > 0 && *s != 0) {
        char tmp[4];
        size_t n = wcrtomb(tmp, *s, ps);
        if (n == (size_t)-1) {
            return (size_t)-1;
        }
        if (len > 0 && count + n > len) {
            return count;
        }
        if (dst != NULL) {
            memcpy(dst + count, tmp, n);
        }
        count += n;
        s++;
        nwc--;
    }
    if (dst != NULL) {
        dst[count] = '\0';
        *src = NULL;
    }
    return count;
}
