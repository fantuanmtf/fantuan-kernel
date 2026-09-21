/* libc-fantuan — regex character classes and intervals (P3): shared by the
 * parser; original code written for libc-fantuan (MIT, not a port). */
#include <ctype.h>
#include <stdlib.h>
#include <string.h>

#include "regex_priv.h"

static void bits_set(unsigned char *bits, int c)
{
    bits[(c & 0xFF) >> 3] |= (unsigned char)(1u << (c & 7));
}

static void bits_set_fold(unsigned char *bits, int c, int icase)
{
    bits_set(bits, c);
    if (icase) {
        bits_set(bits, tolower(c));
        bits_set(bits, toupper(c));
    }
}

static int class_test(const char *name, size_t n, int c)
{
    unsigned char u = (unsigned char)c;
    if (n == 5 && strncmp(name, "alpha", 5) == 0) return isalpha(u);
    if (n == 5 && strncmp(name, "digit", 5) == 0) return isdigit(u);
    if (n == 5 && strncmp(name, "alnum", 5) == 0) return isalnum(u);
    if (n == 5 && strncmp(name, "blank", 5) == 0) return isblank(u);
    if (n == 5 && strncmp(name, "cntrl", 5) == 0) return iscntrl(u);
    if (n == 5 && strncmp(name, "graph", 5) == 0) return isgraph(u);
    if (n == 5 && strncmp(name, "lower", 5) == 0) return islower(u);
    if (n == 5 && strncmp(name, "print", 5) == 0) return isprint(u);
    if (n == 5 && strncmp(name, "punct", 5) == 0) return ispunct(u);
    if (n == 5 && strncmp(name, "space", 5) == 0) return isspace(u);
    if (n == 5 && strncmp(name, "upper", 5) == 0) return isupper(u);
    if (n == 6 && strncmp(name, "xdigit", 6) == 0) return isxdigit(u);
    return 0;
}

/* Parse a bracket expression: *pp points just past '['. Returns the class
 * index into prog->classes, or -1 with *err set on a malformed class. */
int rx_parse_class(const char **pp, int icase, struct rprog *prog, int *err)
{
    const char *p = *pp;
    unsigned char bits[RX_CLASS_BYTES];
    int negate = 0;
    int c;

    memset(bits, 0, sizeof(bits));
    if (*p == '^') {
        negate = 1;
        p++;
    }
    if (*p == ']') {
        bits_set(bits, ']');
        p++;
    }
    while (*p != '\0' && *p != ']') {
        c = (unsigned char)*p++;
        if (c == '[' && *p == ':') {
            const char *start = p + 1;
            const char *end = strstr(start, ":]");
            if (end != NULL) {
                size_t n = (size_t)(end - start);
                for (int i = 0; i < 256; i++) {
                    if (class_test(start, n, i)) {
                        bits_set_fold(bits, i, icase);
                    }
                }
                p = end + 2;
                continue;
            }
        }
        if (c == '\\' && *p != '\0') {
            c = (unsigned char)*p++;
        }
        if (*p == '-' && p[1] != ']' && p[1] != '\0') {
            int hi;
            p++;
            hi = (unsigned char)*p++;
            if (hi == '\\' && *p != '\0') {
                hi = (unsigned char)*p++;
            }
            for (int i = c; i <= hi; i++) {
                bits_set_fold(bits, i, icase);
            }
        } else {
            bits_set_fold(bits, c, icase);
        }
    }
    if (*p != ']') {
        *err = REG_EBRACK;
        return -1;
    }
    p++;
    if (negate) {
        for (int i = 0; i < RX_CLASS_BYTES; i++) {
            bits[i] = (unsigned char)~bits[i];
        }
    }
    if (prog->nclasses >= prog->ccap) {
        int cap = prog->ccap ? prog->ccap * 2 : 4;
        unsigned char *nc = realloc(prog->classes, (size_t)cap * RX_CLASS_BYTES);
        if (nc == NULL) {
            *err = REG_ESPACE;
            return -1;
        }
        prog->classes = nc;
        prog->ccap = cap;
    }
    memcpy(prog->classes + (size_t)prog->nclasses * RX_CLASS_BYTES, bits, RX_CLASS_BYTES);
    *pp = p;
    return prog->nclasses++;
}

/* Parse an interval: *pp points at '{' (ERE) or "\{" (BRE). Returns 1 when
 * a valid interval was consumed, 0 when the text is not an interval (the
 * caller treats the brace literally) and -1 for a malformed one. */
int rx_parse_brace(const char **pp, int extended, int *min, int *max)
{
    const char *p = *pp;
    const char *save = p;
    int lo = 0, hi = -1;
    int seen_hi = 0;

    p += extended ? 1 : 2;
    if (!isdigit((unsigned char)*p)) {
        return 0;
    }
    while (isdigit((unsigned char)*p)) {
        lo = lo * 10 + (*p++ - '0');
    }
    if (*p == ',') {
        p++;
        if (isdigit((unsigned char)*p)) {
            hi = 0;
            seen_hi = 1;
            while (isdigit((unsigned char)*p)) {
                hi = hi * 10 + (*p++ - '0');
            }
        }
    } else {
        hi = lo;
        seen_hi = 1;
    }
    if (extended) {
        if (*p != '}') {
            return 0;
        }
        p++;
    } else {
        if (p[0] != '\\' || p[1] != '}') {
            return 0;
        }
        p += 2;
    }
    if (!seen_hi) {
        hi = -1;
    }
    if (hi >= 0 && hi < lo) {
        *pp = save;
        return -1;
    }
    *pp = p;
    *min = lo;
    *max = hi;
    return 1;
}
