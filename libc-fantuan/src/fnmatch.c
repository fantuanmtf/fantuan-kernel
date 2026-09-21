/* libc-fantuan — fnmatch (P3): shell wildcard matching.
 *
 * An original recursive implementation for libc-fantuan (not a port). It
 * supports '?', '*', bracket expressions with ranges, negation and
 * [:classes:], backslash escaping, and the FNM_PATHNAME / FNM_PERIOD /
 * FNM_NOESCAPE / FNM_LEADING_DIR / FNM_CASEFOLD flags. */
#include <ctype.h>
#include <fnmatch.h>
#include <string.h>

static int fold(int c, int flags)
{
    return (flags & FNM_CASEFOLD) ? tolower(c) : c;
}

static int ch_eq(char a, char b, int flags)
{
    return fold((unsigned char)a, flags) == fold((unsigned char)b, flags);
}

/* Match one bracket expression at *pat (pointing just past '[').
 * Returns 1 on match, 0 otherwise, advances *pat past the closing ']'. */
static int match_bracket(const char **pat, char c, int flags)
{
    const char *p = *pat;
    int negate = 0;
    int matched = 0;

    if (*p == '!' || *p == '^') {
        negate = 1;
        p++;
    }
    if (*p == ']') {
        matched |= ch_eq(']', c, flags);
        p++;
    }
    while (*p != '\0' && *p != ']') {
        if (p[0] == '[' && p[1] == ':') {
            const char *end = strstr(p + 2, ":]");
            if (end != NULL) {
                size_t n = (size_t)(end - (p + 2));
                unsigned char u = (unsigned char)c;
                int hit = 0;
                if (n == 5 && strncmp(p + 2, "alpha", 5) == 0) hit = isalpha(u);
                else if (n == 5 && strncmp(p + 2, "digit", 5) == 0) hit = isdigit(u);
                else if (n == 5 && strncmp(p + 2, "alnum", 5) == 0) hit = isalnum(u);
                else if (n == 5 && strncmp(p + 2, "blank", 5) == 0) hit = isblank(u);
                else if (n == 5 && strncmp(p + 2, "cntrl", 5) == 0) hit = iscntrl(u);
                else if (n == 5 && strncmp(p + 2, "graph", 5) == 0) hit = isgraph(u);
                else if (n == 5 && strncmp(p + 2, "lower", 5) == 0) hit = islower(u);
                else if (n == 5 && strncmp(p + 2, "print", 5) == 0) hit = isprint(u);
                else if (n == 5 && strncmp(p + 2, "punct", 5) == 0) hit = ispunct(u);
                else if (n == 5 && strncmp(p + 2, "space", 5) == 0) hit = isspace(u);
                else if (n == 5 && strncmp(p + 2, "upper", 5) == 0) hit = isupper(u);
                else if (n == 6 && strncmp(p + 2, "xdigit", 6) == 0) hit = isxdigit(u);
                matched |= hit;
                p = end + 2;
                continue;
            }
        }
        {
            char lo = *p++;
            if (lo == '\\' && !(flags & FNM_NOESCAPE) && *p != '\0') {
                lo = *p++;
            }
            if (*p == '-' && p[1] != ']' && p[1] != '\0') {
                char hi = p[1];
                p += 2;
                if (hi == '\\' && !(flags & FNM_NOESCAPE) && *p != '\0') {
                    hi = *p++;
                }
                {
                    unsigned char uc = (unsigned char)fold((unsigned char)c, flags);
                    unsigned char ulo = (unsigned char)fold((unsigned char)lo, flags);
                    unsigned char uhi = (unsigned char)fold((unsigned char)hi, flags);
                    if (uc >= ulo && uc <= uhi) {
                        matched = 1;
                    }
                }
            } else if (ch_eq(lo, c, flags)) {
                matched = 1;
            }
        }
    }
    if (*p == ']') {
        p++;
    }
    *pat = p;
    return negate ? !matched : matched;
}

static int fnmatch_here(const char *p, const char *s, int flags, int period)
{
    for (;;) {
        if (*p == '\0') {
            if (*s == '\0') {
                return 1;
            }
            return (flags & FNM_LEADING_DIR) != 0 && *s == '/';
        }
        if (*p == '\\' && !(flags & FNM_NOESCAPE)) {
            p++;
            if (*p == '\0') {
                return *s == '\\' && s[1] == '\0';
            }
            if (!ch_eq(*p, *s, flags)) {
                return 0;
            }
            period = (*s == '/');
            p++;
            s++;
            continue;
        }
        if (*p == '?') {
            if (*s == '\0' || (period && *s == '.') ||
                ((flags & FNM_PATHNAME) && *s == '/')) {
                return 0;
            }
            p++;
            s++;
            period = 0;
            continue;
        }
        if (*p == '*') {
            while (*p == '*') {
                p++;
            }
            if (*p == '\0') {
                if (period && *s == '.') {
                    return 0;
                }
                if ((flags & FNM_PATHNAME) && strchr(s, '/') != NULL) {
                    return 0;
                }
                return 1;
            }
            for (;;) {
                if (fnmatch_here(p, s, flags, period)) {
                    return 1;
                }
                if (*s == '\0') {
                    return 0;
                }
                if ((flags & FNM_PATHNAME) && *s == '/') {
                    return 0;
                }
                if (period && *s == '.') {
                    return 0;
                }
                period = (*s == '/');
                s++;
            }
        }
        if (*p == '[') {
            if (*s == '\0' || (period && *s == '.') ||
                ((flags & FNM_PATHNAME) && *s == '/')) {
                return 0;
            }
            p++;
            if (!match_bracket(&p, *s, flags)) {
                return 0;
            }
            period = (*s == '/');
            s++;
            continue;
        }
        if (*s == '\0' || !ch_eq(*p, *s, flags)) {
            return 0;
        }
        period = (*s == '/');
        p++;
        s++;
    }
}

int fnmatch(const char *pattern, const char *string, int flags)
{
    return fnmatch_here(pattern, string, flags, (flags & FNM_PERIOD) != 0)
               ? 0
               : FNM_NOMATCH;
}
