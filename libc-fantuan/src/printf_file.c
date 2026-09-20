/* libc-fantuan — FILE printf wrappers (P1).
 *
 * Split from printf.c to keep files within the repo's 300-line convention. */
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>

int vfprintf(FILE *stream, const char *fmt, va_list ap)
{
    char buf[512];
    int n = vsnprintf(buf, sizeof(buf), fmt, ap);
    size_t off = 0;
    size_t total = (size_t)n;
    while (off < total) {
        size_t take = total - off;
        if (take > sizeof(buf) - 1) {
            take = sizeof(buf) - 1;
        }
        if (fwrite(buf + off, 1, take, stream) != take) {
            return -1;
        }
        off += take;
    }
    return n;
}

int fprintf(FILE *stream, const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    int n = vfprintf(stream, fmt, ap);
    va_end(ap);
    return n;
}

int vprintf(const char *fmt, va_list ap)
{
    return vfprintf(stdout, fmt, ap);
}

int printf(const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    int n = vfprintf(stdout, fmt, ap);
    va_end(ap);
    return n;
}
