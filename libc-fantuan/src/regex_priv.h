/* libc-fantuan — internal regex AST/program layout (P3).
 *
 * Not a public header. The parser (regex_parse.c) builds an AST, the
 * emitter (regex_compile.c) turns it into this bytecode and the VM
 * (regex_exec.c) executes it with backtracking. */
#ifndef REGEX_PRIV_H
#define REGEX_PRIV_H

#include <regex.h>
#include <stddef.h>

#define RX_MAXSUB 8          /* captured subexpressions (plus group 0) */
#define RX_CLASS_BYTES 32    /* 256-bit character-class bitmap */

/* AST node types */
enum {
    RX_EMPTY = 0, RX_CHAR, RX_ANY, RX_CLASS, RX_BOL, RX_EOL,
    RX_CAT, RX_ALT, RX_REP, RX_GROUP
};

/* Bytecode opcodes */
enum {
    RX_OP_CHAR = 0, RX_OP_ANY, RX_OP_CLASS, RX_OP_BOL, RX_OP_EOL,
    RX_OP_SAVE, RX_OP_SPLIT, RX_OP_JMP, RX_OP_MATCH
};

struct rnode {
    int type;
    int ch;              /* RX_CHAR */
    int cls;             /* RX_CLASS: index into rprog.classes */
    int min, max;        /* RX_REP: max < 0 means unbounded */
    int cap;             /* RX_GROUP: capture number (1-based) */
    struct rnode *a, *b;
};

struct rinst {
    int op, a, b, c;
};

struct rprog {
    struct rinst *inst;
    int n;
    int cap;
    int nsub;                /* groups seen by the parser */
    unsigned char *classes;  /* nclasses * RX_CLASS_BYTES */
    int nclasses;
    int ccap;
    int icase;               /* REG_ICASE */
    int newline;             /* REG_NEWLINE */
};

struct rnode *rx_new_node(int type);
struct rnode *rx_parse(const char *pattern, int cflags, struct rprog *prog, int *err);
void rx_ast_free(struct rnode *n);
int rx_class_match(const struct rprog *p, int cls, int c);
int rx_parse_class(const char **pp, int icase, struct rprog *prog, int *err);
int rx_parse_brace(const char **pp, int extended, int *min, int *max);

#endif /* REGEX_PRIV_H */
