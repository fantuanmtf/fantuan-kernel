/* libc-fantuan — glob.h (P3): tiny pathname expansion (src/glob.c). */
#ifndef _GLOB_H
#define _GLOB_H

#include <stddef.h>
#include <dirent.h>

typedef struct {
    size_t gl_pathc;  /* number of matched paths */
    char **gl_pathv;  /* NULL-terminated vector of paths */
    size_t gl_offs;   /* slots reserved at the start of gl_pathv with GLOB_DOOFFS */
    int gl_flags;
    void (*gl_closedir)(void *);
    struct dirent *(*gl_readdir)(void *);
    void *(*gl_opendir)(const char *);
    int (*gl_lstat)(const char *, void *);
    int (*gl_stat)(const char *, void *);
} glob_t;

#define GLOB_ERR (1 << 0)
#define GLOB_MARK (1 << 1)
#define GLOB_NOSORT (1 << 2)
#define GLOB_DOOFFS (1 << 3)
#define GLOB_NOCHECK (1 << 4)
#define GLOB_APPEND (1 << 5)
#define GLOB_NOESCAPE (1 << 6)
#define GLOB_PERIOD (1 << 7)
#define GLOB_ALTDIRFUNC (1 << 9)
#define GLOB_BRACE (1 << 10)
#define GLOB_NOMAGIC (1 << 11)
#define GLOB_TILDE (1 << 12)

#define GLOB_NOSPACE 1
#define GLOB_ABORTED 2
#define GLOB_NOMATCH 3
#define GLOB_NOSYS 4

int glob(const char *pattern, int flags, int (*errfunc)(const char *, int), glob_t *pglob);
void globfree(glob_t *pglob);

#endif /* _GLOB_H */
