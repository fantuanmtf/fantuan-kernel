/* libc-fantuan — wordexp.h (P3): shell-style word expansion (src/wordexp.c).
 * Implemented without command substitution: WRDE_NOCMD is accepted, a
 * command substitution in the input fails with WRDE_BADCHAR/CMD cleanly. */
#ifndef _WORDEXP_H
#define _WORDEXP_H

#include <stddef.h>

typedef struct {
    size_t we_wordc;  /* number of words */
    char **we_wordv;  /* NULL-terminated vector of words */
    size_t we_offs;   /* reserved leading slots */
} wordexp_t;

#define WRDE_APPEND (1 << 0)
#define WRDE_DOOFFS (1 << 1)
#define WRDE_NOCMD (1 << 2)
#define WRDE_REUSE (1 << 3)
#define WRDE_SHOWERR (1 << 4)
#define WRDE_UNDEF (1 << 5)

#define WRDE_NOSYS (-1)
#define WRDE_BADCHAR 1
#define WRDE_BADVAL 2
#define WRDE_CMDSUB 3
#define WRDE_NOSPACE 4
#define WRDE_SYNTAX 5

int wordexp(const char *words, wordexp_t *pwordexp, int flags);
void wordfree(wordexp_t *pwordexp);

#endif /* _WORDEXP_H */
