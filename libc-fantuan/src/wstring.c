/* libc-fantuan — wide string helpers and display widths (P3).
 * Split from wchar.c to stay inside the repo's 300-line convention. */
#include <stdlib.h>
#include <string.h>
#include <wchar.h>

size_t wcslen(const wchar_t *s)
{
    size_t n = 0;
    while (s[n] != 0) {
        n++;
    }
    return n;
}

wchar_t *wcscpy(wchar_t *dest, const wchar_t *src)
{
    wchar_t *d = dest;
    while ((*d++ = *src++) != 0) {
    }
    return dest;
}

wchar_t *wcsncpy(wchar_t *dest, const wchar_t *src, size_t n)
{
    size_t i = 0;
    for (; i < n && src[i] != 0; i++) {
        dest[i] = src[i];
    }
    for (; i < n; i++) {
        dest[i] = 0;
    }
    return dest;
}

wchar_t *wcscat(wchar_t *dest, const wchar_t *src)
{
    wcscpy(dest + wcslen(dest), src);
    return dest;
}

int wcscmp(const wchar_t *s1, const wchar_t *s2)
{
    while (*s1 != 0 && *s1 == *s2) {
        s1++;
        s2++;
    }
    return (int)(*s1 - *s2);
}

int wcsncmp(const wchar_t *s1, const wchar_t *s2, size_t n)
{
    while (n > 0 && *s1 != 0 && *s1 == *s2) {
        s1++;
        s2++;
        n--;
    }
    if (n == 0) {
        return 0;
    }
    return (int)(*s1 - *s2);
}

int wcscoll(const wchar_t *s1, const wchar_t *s2)
{
    return wcscmp(s1, s2);
}

wchar_t *wcsdup(const wchar_t *s)
{
    size_t n = wcslen(s) + 1;
    wchar_t *p = malloc(n * sizeof(wchar_t));
    if (p != NULL) {
        wcscpy(p, s);
    }
    return p;
}

wchar_t *wcschr(const wchar_t *s, wchar_t c)
{
    while (*s != 0) {
        if (*s == c) {
            return (wchar_t *)s;
        }
        s++;
    }
    return c == 0 ? (wchar_t *)s : NULL;
}

wchar_t *wcsrchr(const wchar_t *s, wchar_t c)
{
    const wchar_t *last = NULL;
    while (*s != 0) {
        if (*s == c) {
            last = s;
        }
        s++;
    }
    if (c == 0) {
        return (wchar_t *)s;
    }
    return (wchar_t *)last;
}

size_t wcsspn(const wchar_t *s, const wchar_t *accept)
{
    size_t n = 0;
    while (s[n] != 0 && wcschr(accept, s[n]) != NULL) {
        n++;
    }
    return n;
}

size_t wcscspn(const wchar_t *s, const wchar_t *reject)
{
    size_t n = 0;
    while (s[n] != 0 && wcschr(reject, s[n]) == NULL) {
        n++;
    }
    return n;
}

/* Minimal Unicode-width table: C0/C1 are -1, combining marks are 0 and the
 * common East Asian wide blocks are 2; everything else is 1. */
wchar_t *wmemchr(const wchar_t *s, wchar_t c, size_t n)
{
    for (size_t i = 0; i < n; i++) {
        if (s[i] == c) {
            return (wchar_t *)(s + i);
        }
    }
    return NULL;
}

int wmemcmp(const wchar_t *s1, const wchar_t *s2, size_t n)
{
    for (size_t i = 0; i < n; i++) {
        if (s1[i] != s2[i]) {
            return s1[i] < s2[i] ? -1 : 1;
        }
    }
    return 0;
}

wchar_t *wmemcpy(wchar_t *dest, const wchar_t *src, size_t n)
{
    for (size_t i = 0; i < n; i++) {
        dest[i] = src[i];
    }
    return dest;
}

wchar_t *wmemmove(wchar_t *dest, const wchar_t *src, size_t n)
{
    if (dest < src) {
        for (size_t i = 0; i < n; i++) {
            dest[i] = src[i];
        }
    } else if (dest > src) {
        for (size_t i = n; i > 0; i--) {
            dest[i - 1] = src[i - 1];
        }
    }
    return dest;
}

wchar_t *wmemset(wchar_t *s, wchar_t c, size_t n)
{
    for (size_t i = 0; i < n; i++) {
        s[i] = c;
    }
    return s;
}

int wcwidth(wchar_t wc)
{
    unsigned int c = (unsigned int)wc;
    if (c == 0) return 0;
    if (c < 32 || (c >= 0x7F && c < 0xA0)) return -1;
    if (c >= 0x0300 && c <= 0x036F) return 0;
    if (c >= 0x0483 && c <= 0x0489) return 0;
    if (c >= 0x1AB0 && c <= 0x1AFF) return 0;
    if (c >= 0x1DC0 && c <= 0x1DFF) return 0;
    if (c >= 0x20D0 && c <= 0x20FF) return 0;
    if (c >= 0xFE20 && c <= 0xFE2F) return 0;
    if (c >= 0x1100 && c <= 0x115F) return 2;
    if (c >= 0x2E80 && c <= 0xA4CF) return 2;
    if (c >= 0xAC00 && c <= 0xD7A3) return 2;
    if (c >= 0xF900 && c <= 0xFAFF) return 2;
    if (c >= 0xFE30 && c <= 0xFE4F) return 2;
    if (c >= 0xFF00 && c <= 0xFF60) return 2;
    if (c >= 0xFFE0 && c <= 0xFFE6) return 2;
    if (c >= 0x1F300 && c <= 0x1FAFF) return 2;
    if (c >= 0x20000 && c <= 0x3FFFD) return 2;
    return 1;
}

int wcswidth(const wchar_t *s, size_t n)
{
    int total = 0;
    for (size_t i = 0; i < n && s[i] != 0; i++) {
        int w = wcwidth(s[i]);
        if (w < 0) {
            return -1;
        }
        total += w;
    }
    return total;
}
