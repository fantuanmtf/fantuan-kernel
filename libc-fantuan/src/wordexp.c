/* libc-fantuan — wordexp (P3): shell-style word splitting with quoting and
 * environment expansion. Command substitution and pathname expansion are
 * not performed (glob characters stay literal, `$(`/backquote fails with
 * WRDE_CMDSUB); bash does not rely on wordexp for its own expansions. */
#include <ctype.h>
#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <wordexp.h>

struct wx {
    char **v;
    size_t n;
    size_t cap;
    int rc;
};

static int wx_push(struct wx *x, char **cur, size_t *clen, int *have_word)
{
    char *w;
    if (!*have_word) {
        return 0;
    }
    w = malloc(*clen + 1);
    if (w == NULL) {
        return -1;
    }
    memcpy(w, *cur, *clen);
    w[*clen] = '\0';
    if (x->n + 1 >= x->cap) {
        size_t cap = x->cap ? x->cap * 2 : 8;
        char **nv = realloc(x->v, cap * sizeof(char *));
        if (nv == NULL) {
            free(w);
            return -1;
        }
        x->v = nv;
        x->cap = cap;
    }
    x->v[x->n++] = w;
    *clen = 0;
    *have_word = 0;
    return 0;
}

static int append_str(char *cur, size_t *clen, size_t max, const char *s)
{
    size_t n = strlen(s);
    if (*clen + n > max) {
        n = max - *clen;
    }
    memcpy(cur + *clen, s, n);
    *clen += n;
    return 0;
}

static int expand_var(const char **pp, int flags, struct wx *x, char **cur, size_t *clen,
                      int *have_word, size_t max, int split)
{
    const char *p = *pp;
    char name[128];
    size_t nl = 0;
    const char *val;
    if (p[1] == '{') {
        const char *end;
        p += 2;
        end = strchr(p, '}');
        if (end == NULL) {
            return WRDE_SYNTAX;
        }
        nl = (size_t)(end - p);
        if (nl >= sizeof(name)) {
            return WRDE_BADVAL;
        }
        memcpy(name, p, nl);
        name[nl] = '\0';
        p = end + 1;
    } else {
        p++;
        while ((isalnum((unsigned char)*p) || *p == '_') && nl + 1 < sizeof(name)) {
            name[nl++] = *p++;
        }
        name[nl] = '\0';
        if (nl == 0) {
            (*cur)[(*clen)++] = '$';
            *have_word = 1;
            *pp = p;
            return 0;
        }
    }
    val = getenv(name);
    if (val == NULL) {
        if (flags & WRDE_UNDEF) {
            return WRDE_BADVAL;
        }
        val = "";
    }
    if (split) {
        /* Unquoted expansions undergo field splitting (IFS whitespace). */
        const char *q = val;
        while (*q != '\0') {
            if (isspace((unsigned char)*q)) {
                if (*clen > 0 || *have_word) {
                    if (wx_push(x, cur, clen, have_word) != 0) {
                        return WRDE_NOSPACE;
                    }
                }
            } else {
                if (*clen < max) {
                    (*cur)[(*clen)++] = *q;
                }
                *have_word = 1;
            }
            q++;
        }
    } else {
        append_str(*cur, clen, max, val);
        *have_word = 1;
    }
    *pp = p;
    return 0;
}

int wordexp(const char *words, wordexp_t *pwordexp, int flags)
{
    struct wx x = {NULL, 0, 0, 0};
    char *cur;
    size_t clen = 0;
    size_t max;
    int have_word = 0;
    int in_single = 0;
    int in_double = 0;
    const char *p = words;
    size_t i;
    int rc = 0;

    if (words == NULL || pwordexp == NULL) {
        return WRDE_SYNTAX;
    }
    max = strlen(words) + 1;
    cur = malloc(max + 1);
    if (cur == NULL) {
        return WRDE_NOSPACE;
    }
    while (*p != '\0' && rc == 0) {
        char c = *p;
        if (in_single) {
            if (c == '\'') {
                in_single = 0;
            } else {
                cur[clen++] = c;
            }
            p++;
            continue;
        }
        if (in_double) {
            if (c == '"') {
                in_double = 0;
                p++;
                continue;
            }
            if (c == '\\') {
                p++;
                if (*p != '\0') {
                    cur[clen++] = *p++;
                }
                continue;
            }
            if (c == '$' && p[1] == '(') {
                rc = WRDE_CMDSUB;
                break;
            }
            if (c == '$') {
                rc = expand_var(&p, flags, &x, &cur, &clen, &have_word, max, 0);
                continue;
            }
            if (c == '`') {
                rc = WRDE_CMDSUB;
                break;
            }
            cur[clen++] = c;
            p++;
            continue;
        }
        if (isspace((unsigned char)c)) {
            if (wx_push(&x, &cur, &clen, &have_word) != 0) {
                rc = WRDE_NOSPACE;
            }
            p++;
            continue;
        }
        if (c == '\'') {
            in_single = 1;
            have_word = 1;
            p++;
            continue;
        }
        if (c == '"') {
            in_double = 1;
            have_word = 1;
            p++;
            continue;
        }
        if (c == '\\') {
            p++;
            if (*p != '\0') {
                cur[clen++] = *p++;
                have_word = 1;
            }
            continue;
        }
        if (c == '`' || (c == '$' && p[1] == '(')) {
            rc = WRDE_CMDSUB;
            break;
        }
        if (c == '$') {
            rc = expand_var(&p, flags, &x, &cur, &clen, &have_word, max, 1);
            continue;
        }
        if (c == '~' && clen == 0 && !have_word) {
            const char *home = getenv("HOME");
            if (home != NULL) {
                append_str(cur, &clen, max, home);
                have_word = 1;
                p++;
                continue;
            }
        }
        cur[clen++] = c;
        have_word = 1;
        p++;
    }
    if (rc == 0 && (in_single || in_double)) {
        rc = WRDE_SYNTAX;
    }
    if (rc == 0 && wx_push(&x, &cur, &clen, &have_word) != 0) {
        rc = WRDE_NOSPACE;
    }
    free(cur);

    if (rc != 0) {
        for (i = 0; i < x.n; i++) {
            free(x.v[i]);
        }
        free(x.v);
        return rc;
    }

    {
        size_t base = (flags & WRDE_APPEND) ? pwordexp->we_wordc : 0;
        size_t offs = (flags & WRDE_DOOFFS) ? pwordexp->we_offs : 0;
        char **out;
        if (flags & WRDE_APPEND) {
            out = realloc(pwordexp->we_wordv, (base + offs + x.n + 1) * sizeof(char *));
        } else {
            if (flags & WRDE_REUSE) {
                free(pwordexp->we_wordv);
            }
            out = malloc((base + offs + x.n + 1) * sizeof(char *));
        }
        if (out == NULL) {
            for (i = 0; i < x.n; i++) {
                free(x.v[i]);
            }
            free(x.v);
            return WRDE_NOSPACE;
        }
        if (!(flags & WRDE_APPEND)) {
            base = 0;
            for (i = 0; i < offs; i++) {
                out[i] = NULL;
            }
        }
        for (i = 0; i < x.n; i++) {
            out[base + offs + i] = x.v[i];
        }
        out[base + offs + x.n] = NULL;
        pwordexp->we_wordv = out;
        pwordexp->we_wordc = base + x.n;
        /* wordfree() has no flags: record the leading slot count so it never
         * reads a caller's uninitialised we_offs (0 when WRDE_DOOFFS is off). */
        pwordexp->we_offs = offs;
    }
    free(x.v);
    return 0;
}

void wordfree(wordexp_t *pwordexp)
{
    size_t i;
    size_t offs;
    if (pwordexp == NULL || pwordexp->we_wordv == NULL) {
        return;
    }
    offs = (pwordexp->we_offs != 0) ? pwordexp->we_offs : 0;
    for (i = 0; i < pwordexp->we_wordc; i++) {
        free(pwordexp->we_wordv[offs + i]);
    }
    free(pwordexp->we_wordv);
    pwordexp->we_wordv = NULL;
    pwordexp->we_wordc = 0;
    pwordexp->we_offs = 0;
}
