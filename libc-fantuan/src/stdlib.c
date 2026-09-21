/* libc-fantuan — stdlib (P1): conversions, environment, exit, qsort. */
#include <ctype.h>
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fantuan/abi.h>

int main(int argc, char **argv);

void _exit(int status)
{
    __fantuan_syscall6(FANTUAN_SYS_EXIT, status, 0, 0, 0, 0);
    for (;;) {
        /* task::exit never returns */
    }
}

void _Exit(int status)
{
    _exit(status);
}

void exit(int status)
{
    extern void __stdio_flush_all(void);
    __stdio_flush_all();
    _exit(status);
}

void abort(void)
{
    static const char msg[] = "abort\n";
    ssize_t r = write(2, msg, sizeof(msg) - 1);
    (void)r;
    _exit(127);
}

void __assert_fail(const char *expr, const char *file, unsigned int line, const char *func)
{
    (void)func;
    fprintf(stderr, "%s:%u: assertion failed: %s\n", file, line, expr);
    abort();
}

int atexit(void (*func)(void))
{
    (void)func; /* P1: no atexit list yet (exit still flushes stdio) */
    return 0;
}

int atoi(const char *nptr) { return (int)strtol(nptr, NULL, 10); }
long atol(const char *nptr) { return strtol(nptr, NULL, 10); }
long long atoll(const char *nptr) { return strtoll(nptr, NULL, 10); }
double atof(const char *nptr) { return strtod(nptr, NULL); }

static int digit_val(int c)
{
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'z') return c - 'a' + 10;
    if (c >= 'A' && c <= 'Z') return c - 'A' + 10;
    return -1;
}

unsigned long long strtoull(const char *nptr, char **endptr, int base)
{
    const char *s = nptr;
    while (isspace((unsigned char)*s)) s++;
    int neg = 0;
    if (*s == '+' || *s == '-') {
        neg = (*s == '-');
        s++;
    }
    const char *digits = s;
    if ((base == 0 || base == 16) && s[0] == '0' && (s[1] == 'x' || s[1] == 'X')) {
        s += 2;
        base = 16;
    } else if (base == 0) {
        /* base 0 means "infer": a leading 0 is octal, otherwise decimal.
         * The leading 0 is left in place: it is itself an octal digit. */
        base = s[0] == '0' ? 8 : 10;
    }
    unsigned long long acc = 0;
    int any = 0;
    for (;; s++) {
        int d = digit_val((unsigned char)*s);
        if (d < 0 || d >= base) break;
        acc = acc * (unsigned long long)base + (unsigned long long)d;
        any = 1;
    }
    if (endptr) {
        if (any) {
            *endptr = (char *)s;
        } else if (base == 16 && s != digits) {
            *endptr = (char *)(digits + 1); /* "0x" with no hex digits: after the 0 */
        } else {
            *endptr = (char *)nptr; /* no conversion at all */
        }
    }
    return neg ? (unsigned long long)(-(long long)acc) : acc;
}

long long strtoll(const char *nptr, char **endptr, int base)
{
    const char *s = nptr;
    while (isspace((unsigned char)*s)) s++;
    int neg = (*s == '-');
    unsigned long long v = strtoull(nptr, endptr, base);
    if (neg) {
        if (v > (unsigned long long)LLONG_MAX + 1ULL) {
            errno = ERANGE;
            return LLONG_MIN;
        }
        return (long long)(-v);
    }
    if (v > (unsigned long long)LLONG_MAX) {
        errno = ERANGE;
        return LLONG_MAX;
    }
    return (long long)v;
}

unsigned long strtoul(const char *nptr, char **endptr, int base)
{
    return (unsigned long)strtoull(nptr, endptr, base);
}

long strtol(const char *nptr, char **endptr, int base)
{
    return (long)strtoll(nptr, endptr, base);
}

double strtod(const char *nptr, char **endptr)
{
    const char *s = nptr;
    while (isspace((unsigned char)*s)) s++;
    int neg = 0;
    if (*s == '+' || *s == '-') {
        neg = (*s == '-');
        s++;
    }
    double val = 0.0;
    int any = 0;
    while (*s >= '0' && *s <= '9') {
        val = val * 10.0 + (double)(*s - '0');
        s++;
        any = 1;
    }
    if (*s == '.') {
        s++;
        double scale = 0.1;
        while (*s >= '0' && *s <= '9') {
            val += (double)(*s - '0') * scale;
            scale *= 0.1;
            s++;
            any = 1;
        }
    }
    if (*s == 'e' || *s == 'E') {
        const char *e = s + 1;
        int eneg = 0;
        if (*e == '+' || *e == '-') {
            eneg = (*e == '-');
            e++;
        }
        int exp = 0;
        while (*e >= '0' && *e <= '9') {
            exp = exp * 10 + (*e - '0');
            e++;
        }
        while (exp-- > 0) {
            val = eneg ? val / 10.0 : val * 10.0;
        }
        s = e;
    }
    if (endptr) *endptr = (char *)(any ? s : nptr);
    return neg ? -val : val;
}
