/* libc-fantuan — regex.h (P3): POSIX regcomp/regexec/regfree, an original
 * compact backtracking engine written for libc-fantuan (not a port): a
 * recursive-descent parser builds a small AST (src/regex_compile.c) and a
 * depth-first bytecode VM executes it (src/regex_exec.c). Supported: BRE
 * and ERE literals, '.', '^', '$', bracket expressions with [:classes:],
 * '*', '+', '?', '{m,n}', grouping and alternation, REG_ICASE/REG_NEWLINE.
 * Back-references are not supported (BRE \1 is treated as a literal).
 * Matches are leftmost; subexpressions follow greedy backtracking order.
 * The engine lives in-repo under libc-fantuan's MIT licence. */
#ifndef _REGEX_H
#define _REGEX_H

#include <stddef.h>

typedef long regoff_t;

typedef struct {
    size_t re_nsub;   /* number of parenthesised subexpressions */
    void *__prog;     /* internal compiled program (malloc'd) */
    int __cflags;
} regex_t;

typedef struct {
    regoff_t rm_so;   /* byte offset of match start, -1 if no match */
    regoff_t rm_eo;   /* byte offset one past the match end */
} regmatch_t;

/* regcomp flags */
#define REG_EXTENDED 1
#define REG_ICASE 2
#define REG_NEWLINE 4
#define REG_NOSUB 8

/* regexec flags */
#define REG_NOTBOL 1
#define REG_NOTEOL 2

/* error codes (regerror takes these) */
#define REG_NOMATCH 1
#define REG_BADPAT 2
#define REG_ECOLLATE 3
#define REG_ECTYPE 4
#define REG_EESCAPE 5
#define REG_ESUBREG 6
#define REG_EBRACK 7
#define REG_EPAREN 8
#define REG_EBRACE 9
#define REG_BADBR 10
#define REG_ERANGE 11
#define REG_ESPACE 12
#define REG_BADRPT 13
#define REG_ENOSYS 14

int regcomp(regex_t *preg, const char *regex, int cflags);
int regexec(const regex_t *preg, const char *string, size_t nmatch,
            regmatch_t pmatch[], int eflags);
size_t regerror(int errcode, const regex_t *preg, char *errbuf,
                size_t errbuf_size);
void regfree(regex_t *preg);

#endif /* _REGEX_H */
