/* libc-fantuan — printf core (P1).
 *
 * One formatter feeds printf/fprintf/sprintf/snprintf and the v* variants.
 * Supported: flags -+ #0, width and precision (numeric and *), length
 * hh/h/l/ll/z/j/t, conversions d i u o x X p c s %. Locale-independent. */
#include <stdarg.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>

struct out {
    char *buf;   /* NULL: discard */
    size_t cap;  /* buffer capacity (including NUL) */
    size_t len;  /* total bytes that would be written */
};

static void emit(struct out *o, char c)
{
    if (o->buf && o->cap > 0 && o->len + 1 < o->cap) {
        o->buf[o->len] = c;
    }
    o->len++;
}

static void emit_str(struct out *o, const char *s, int max)
{
    int n = 0;
    while (*s && (max < 0 || n < max)) {
        emit(o, *s++);
        n++;
    }
}

static void emit_pad(struct out *o, char c, int count)
{
    while (count-- > 0) {
        emit(o, c);
    }
}

static void emit_num(struct out *o, unsigned long long v, unsigned base, int upper,
                     int width, int left, int zero, int neg, int plus, int space,
                     int alt, int prec)
{
    char tmp[70];
    const char *digits = upper ? "0123456789ABCDEF" : "0123456789abcdef";
    int n = 0;
    if (v == 0) {
        tmp[n++] = '0';
    }
    while (v) {
        tmp[n++] = digits[v % base];
        v /= base;
    }
    int prefix = (neg || plus || space) ? 1 : 0;
    int alt_prefix = 0;
    if (alt && base == 16 && !(n == 1 && tmp[0] == '0')) {
        alt_prefix = 2;
    } else if (alt && base == 8 && !(n == 1 && tmp[0] == '0')) {
        alt_prefix = 1;
    }
    int zeros = (prec > n) ? prec - n : 0;
    if (zero && !left && prec < 0) {
        int used = prefix + alt_prefix + n;
        zeros = width > used ? width - used : 0;
    }
    int body = prefix + alt_prefix + n + zeros;
    if (!left) {
        emit_pad(o, zero && prec < 0 ? '0' : ' ', width - body);
    }
    if (prefix) {
        emit(o, neg ? '-' : (plus ? '+' : ' '));
    }
    if (alt_prefix == 2) {
        emit(o, '0');
        emit(o, upper ? 'X' : 'x');
    } else if (alt_prefix == 1) {
        emit(o, '0');
    }
    emit_pad(o, '0', zeros);
    while (n > 0) {
        emit(o, tmp[--n]);
    }
    if (left) {
        emit_pad(o, ' ', width - body);
    }
}

static int vsnprintf_core(struct out *o, const char *fmt, va_list ap)
{
    while (*fmt) {
        if (*fmt != '%') {
            emit(o, *fmt++);
            continue;
        }
        fmt++;
        int left = 0, plus = 0, space = 0, alt = 0, zero = 0;
        for (;; fmt++) {
            if (*fmt == '-') left = 1;
            else if (*fmt == '+') plus = 1;
            else if (*fmt == ' ') space = 1;
            else if (*fmt == '#') alt = 1;
            else if (*fmt == '0') zero = 1;
            else break;
        }
        int width = 0;
        if (*fmt == '*') {
            width = va_arg(ap, int);
            if (width < 0) {
                left = 1;
                width = -width;
            }
            fmt++;
        } else {
            while (*fmt >= '0' && *fmt <= '9') {
                width = width * 10 + (*fmt++ - '0');
            }
        }
        int prec = -1;
        if (*fmt == '.') {
            fmt++;
            prec = 0;
            if (*fmt == '*') {
                prec = va_arg(ap, int);
                fmt++;
            } else {
                while (*fmt >= '0' && *fmt <= '9') {
                    prec = prec * 10 + (*fmt++ - '0');
                }
            }
        }
        /* length modifiers */
        int lcount = 0;
        while (*fmt == 'h' || *fmt == 'l' || *fmt == 'z' || *fmt == 'j' || *fmt == 't' ||
               *fmt == 'L') {
            if (*fmt == 'h' || *fmt == 'l') {
                lcount++;
            } else {
                lcount = 2;
            }
            fmt++;
        }
        switch (*fmt) {
        case 'd':
        case 'i': {
            long long v;
            if (lcount >= 2) {
                v = va_arg(ap, long long);
            } else if (lcount == 1) {
                v = va_arg(ap, long);
            } else {
                v = va_arg(ap, int);
            }
            int neg = v < 0;
            unsigned long long u = neg ? (unsigned long long)(-(v + 1)) + 1ull
                                       : (unsigned long long)v;
            emit_num(o, u, 10, 0, width, left, zero, neg, plus, space, 0, prec);
            break;
        }
        case 'u':
        case 'o':
        case 'x':
        case 'X': {
            unsigned long long v;
            if (lcount >= 2) {
                v = va_arg(ap, unsigned long long);
            } else if (lcount == 1) {
                v = va_arg(ap, unsigned long);
            } else {
                v = va_arg(ap, unsigned int);
            }
            int base = (*fmt == 'o') ? 8 : ((*fmt == 'u') ? 10 : 16);
            emit_num(o, v, (unsigned)base, *fmt == 'X', width, left, zero, 0, 0, 0, alt,
                     prec);
            break;
        }
        case 'p': {
            void *p = va_arg(ap, void *);
            emit_str(o, "0x", -1);
            emit_num(o, (unsigned long long)(unsigned long)p, 16, 0, 0, 0, 0, 0, 0, 0, 0,
                     -1);
            break;
        }
        case 'c': {
            char c = (char)va_arg(ap, int);
            if (!left && width > 1) {
                emit_pad(o, ' ', width - 1);
            }
            emit(o, c);
            if (left && width > 1) {
                emit_pad(o, ' ', width - 1);
            }
            break;
        }
        case 's': {
            const char *s = va_arg(ap, const char *);
            if (!s) {
                s = "(null)";
            }
            int slen = 0;
            while (s[slen] && (prec < 0 || slen < prec)) {
                slen++;
            }
            if (!left && width > slen) {
                emit_pad(o, ' ', width - slen);
            }
            emit_str(o, s, prec);
            if (left && width > slen) {
                emit_pad(o, ' ', width - slen);
            }
            break;
        }
        case '%':
            emit(o, '%');
            break;
        case 0:
            return (int)o->len;
        default:
            emit(o, '%');
            emit(o, *fmt);
            break;
        }
        fmt++;
    }
    return (int)o->len;
}

int vsnprintf(char *buf, size_t size, const char *fmt, va_list ap)
{
    struct out o = { size > 0 ? buf : NULL, size, 0 };
    int n = vsnprintf_core(&o, fmt, ap);
    if (size > 0) {
        size_t end = (o.len < size - 1) ? o.len : size - 1;
        buf[end] = 0;
    }
    return n;
}

int snprintf(char *buf, size_t size, const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(buf, size, fmt, ap);
    va_end(ap);
    return n;
}

int sprintf(char *buf, const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(buf, (size_t)-1, fmt, ap);
    va_end(ap);
    return n;
}

int vsprintf(char *buf, const char *fmt, va_list ap)
{
    return vsnprintf(buf, (size_t)-1, fmt, ap);
}
