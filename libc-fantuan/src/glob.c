/* libc-fantuan — glob (P3): pathname expansion over fnmatch.
 *
 * An original, deliberately small implementation for libc-fantuan (not a
 * port): the pattern is split on '/', each component is expanded against
 * readdir() (or matched literally for stat()), and the final vector is
 * sorted. Supports GLOB_ERR, GLOB_MARK, GLOB_NOSORT, GLOB_DOOFFS,
 * GLOB_NOCHECK, GLOB_APPEND and GLOB_NOESCAPE. Braces/tilde are not
 * expanded by this layer; bash uses its own bundled glob for those. */
#include <dirent.h>
#include <errno.h>
#include <fnmatch.h>
#include <glob.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

struct vec {
    char **v;
    size_t n;
    size_t cap;
};

static int vec_push(struct vec *x, char *s)
{
    if (x->n + 1 >= x->cap) {
        size_t cap = x->cap ? x->cap * 2 : 8;
        char **nv = realloc(x->v, cap * sizeof(char *));
        if (nv == NULL) {
            return -1;
        }
        x->v = nv;
        x->cap = cap;
    }
    x->v[x->n++] = s;
    return 0;
}

static void vec_free(struct vec *x)
{
    for (size_t i = 0; i < x->n; i++) {
        free(x->v[i]);
    }
    free(x->v);
    x->v = NULL;
    x->n = x->cap = 0;
}

static char *path_join(const char *dir, const char *name)
{
    size_t dl = strlen(dir);
    size_t nl = strlen(name);
    int slash = (dl > 0 && dir[dl - 1] == '/');
    char *out = malloc(dl + (slash ? 0 : 1) + nl + 1);
    if (out == NULL) {
        return NULL;
    }
    memcpy(out, dir, dl);
    if (!slash) {
        out[dl++] = '/';
    }
    memcpy(out + dl, name, nl + 1);
    return out;
}

static int has_magic(const char *s)
{
    return strpbrk(s, "*?[") != NULL;
}

static int is_dir(const char *path)
{
    struct stat st;
    return stat(path, &st) == 0 && S_ISDIR(st.st_mode);
}

static int exists(const char *path)
{
    struct stat st;
    return stat(path, &st) == 0;
}

static int expand_component(struct vec *out, const char *prefix, const char *comp,
                            int flags, int is_last, int (*errfunc)(const char *, int),
                            const char *whole)
{
    if (!has_magic(comp)) {
        char *joined = path_join(prefix, comp);
        if (joined == NULL) {
            return -1;
        }
        if (exists(joined)) {
            if (is_last || is_dir(joined) || (flags & GLOB_MARK)) {
                if (vec_push(out, joined) != 0) {
                    free(joined);
                    return -1;
                }
                return 0;
            }
        }
        free(joined);
        if (is_last && (flags & GLOB_NOCHECK)) {
            char *p = strdup(whole);
            if (p == NULL || vec_push(out, p) != 0) {
                free(p);
                return -1;
            }
        }
        return 0;
    }

    const char *dirpath = (prefix[0] != '\0') ? prefix : ".";
    DIR *d = opendir(dirpath);
    if (d == NULL) {
        if (errfunc != NULL && errfunc(dirpath, errno) != 0) {
            return 1;
        }
        return 0;
    }
    struct dirent *de;
    while ((de = readdir(d)) != NULL) {
        char *joined;
        int mflags = (flags & GLOB_NOESCAPE) ? FNM_NOESCAPE : 0;
        if (de->d_name[0] == '.' && comp[0] != '.' && !(flags & GLOB_PERIOD)) {
            continue;
        }
        if (fnmatch(comp, de->d_name, mflags) != 0) {
            continue;
        }
        joined = path_join(prefix, de->d_name);
        if (joined == NULL) {
            closedir(d);
            return -1;
        }
        if (is_last || is_dir(joined)) {
            if (vec_push(out, joined) != 0) {
                free(joined);
                closedir(d);
                return -1;
            }
        } else {
            free(joined);
        }
    }
    closedir(d);
    return 0;
}

static int cmp_str(const void *a, const void *b)
{
    return strcmp(*(const char *const *)a, *(const char *const *)b);
}

int glob(const char *pattern, int flags, int (*errfunc)(const char *, int), glob_t *pglob)
{
    struct vec cur = {0, 0, 0}, next = {0, 0, 0};
    char *work = strdup(pattern);
    char **comps;
    size_t ncomp = 0;
    if (work == NULL) {
        return GLOB_NOSPACE;
    }
    comps = malloc((strlen(pattern) + 1) * sizeof(char *));
    if (comps == NULL) {
        free(work);
        return GLOB_NOSPACE;
    }
    {
        char *save = NULL;
        for (char *t = strtok_r(work, "/", &save); t != NULL; t = strtok_r(NULL, "/", &save)) {
            comps[ncomp++] = t;
        }
    }

    if (vec_push(&cur, strdup(pattern[0] == '/' ? "/" : "")) != 0) {
        free(comps);
        free(work);
        return GLOB_NOSPACE;
    }

    for (size_t ci = 0; ci < ncomp && cur.n > 0; ci++) {
        int last = (ci + 1 == ncomp);
        for (size_t i = 0; i < cur.n; i++) {
            int r = expand_component(&next, cur.v[i], comps[ci], flags, last, errfunc, pattern);
            if (r == 1) {
                free(comps);
                vec_free(&cur);
                vec_free(&next);
                free(work);
                return GLOB_ABORTED;
            }
            if (r < 0) {
                free(comps);
                vec_free(&cur);
                vec_free(&next);
                free(work);
                return GLOB_NOSPACE;
            }
        }
        vec_free(&cur);
        cur = next;
        next.v = NULL;
        next.n = next.cap = 0;
    }

    free(comps);
    free(work);

    if (cur.n == 0) {
        if (!(flags & GLOB_NOCHECK)) {
            vec_free(&cur);
            return GLOB_NOMATCH;
        }
        if (vec_push(&cur, strdup(pattern)) != 0) {
            vec_free(&cur);
            return GLOB_NOSPACE;
        }
    }

    if (!(flags & GLOB_NOSORT)) {
        qsort(cur.v, cur.n, sizeof(char *), cmp_str);
    }

    size_t offs = (flags & GLOB_DOOFFS) ? pglob->gl_offs : 0;
    size_t base = (flags & GLOB_APPEND) ? pglob->gl_pathc : 0;
    char **nv = realloc(pglob->gl_pathv, (base + offs + cur.n + 1) * sizeof(char *));
    if (nv == NULL) {
        vec_free(&cur);
        return GLOB_NOSPACE;
    }
    if (!(flags & GLOB_APPEND)) {
        for (size_t i = 0; i < offs; i++) {
            nv[i] = NULL;
        }
    }
    pglob->gl_pathv = nv;
    size_t at = base + offs;
    for (size_t i = 0; i < cur.n; i++) {
        pglob->gl_pathv[at + i] = cur.v[i];
    }
    pglob->gl_pathv[at + cur.n] = NULL;
    pglob->gl_pathc = base + cur.n;
    pglob->gl_flags = flags;
    free(cur.v);
    return 0;
}

void globfree(glob_t *pglob)
{
    if (pglob == NULL || pglob->gl_pathv == NULL) {
        return;
    }
    size_t offs = (pglob->gl_flags & GLOB_DOOFFS) ? pglob->gl_offs : 0;
    for (size_t i = 0; i < pglob->gl_pathc; i++) {
        free(pglob->gl_pathv[offs + i]);
    }
    free(pglob->gl_pathv);
    pglob->gl_pathv = NULL;
    pglob->gl_pathc = 0;
}
