/* libc-fantuan — regex emitter/regcomp (P3): turns the AST from
 * regex_parse.c into the backtracking bytecode of regex_priv.h.
 *
 * Original code written for libc-fantuan (MIT, not a port). */
#include <stdlib.h>
#include <string.h>

#include "regex_priv.h"

struct rnode *rx_new_node(int type)
{
    struct rnode *n = calloc(1, sizeof(*n));
    if (n != NULL) {
        n->type = type;
        n->max = -1;
    }
    return n;
}

void rx_ast_free(struct rnode *n)
{
    if (n == NULL) {
        return;
    }
    rx_ast_free(n->a);
    rx_ast_free(n->b);
    free(n);
}

static int push(struct rprog *p, int op, int a, int b, int c)
{
    if (p->n >= p->cap) {
        int cap = p->cap ? p->cap * 2 : 64;
        struct rinst *ni = realloc(p->inst, (size_t)cap * sizeof(*ni));
        if (ni == NULL) {
            return -1;
        }
        p->inst = ni;
        p->cap = cap;
    }
    p->inst[p->n].op = op;
    p->inst[p->n].a = a;
    p->inst[p->n].b = b;
    p->inst[p->n].c = c;
    return p->n++;
}

static int emit(struct rprog *p, struct rnode *n);

static int emit_rep(struct rprog *p, struct rnode *n)
{
    int i;
    for (i = 0; i < n->min; i++) {
        if (emit(p, n->a) != 0) {
            return -1;
        }
    }
    if (n->max < 0) {
        int l1 = p->n;
        int s = push(p, RX_OP_SPLIT, 0, 0, 0);
        if (s < 0) {
            return -1;
        }
        p->inst[s].a = p->n;
        if (emit(p, n->a) != 0) {
            return -1;
        }
        if (push(p, RX_OP_JMP, l1, 0, 0) < 0) {
            return -1;
        }
        p->inst[s].b = p->n;
    } else {
        for (i = n->min; i < n->max; i++) {
            int s = push(p, RX_OP_SPLIT, 0, 0, 0);
            if (s < 0) {
                return -1;
            }
            p->inst[s].a = p->n;
            if (emit(p, n->a) != 0) {
                return -1;
            }
            p->inst[s].b = p->n;
        }
    }
    return 0;
}

static int emit(struct rprog *p, struct rnode *n)
{
    switch (n->type) {
    case RX_EMPTY:
        return 0;
    case RX_CHAR:
        return push(p, RX_OP_CHAR, n->ch, 0, 0) < 0 ? -1 : 0;
    case RX_ANY:
        return push(p, RX_OP_ANY, 0, 0, 0) < 0 ? -1 : 0;
    case RX_CLASS:
        return push(p, RX_OP_CLASS, n->cls, 0, 0) < 0 ? -1 : 0;
    case RX_BOL:
        return push(p, RX_OP_BOL, 0, 0, 0) < 0 ? -1 : 0;
    case RX_EOL:
        return push(p, RX_OP_EOL, 0, 0, 0) < 0 ? -1 : 0;
    case RX_CAT:
        return emit(p, n->a) != 0 || emit(p, n->b) != 0 ? -1 : 0;
    case RX_ALT: {
        int j;
        int s = push(p, RX_OP_SPLIT, 0, 0, 0);
        if (s < 0) {
            return -1;
        }
        p->inst[s].a = p->n;
        if (emit(p, n->a) != 0) {
            return -1;
        }
        j = push(p, RX_OP_JMP, 0, 0, 0);
        if (j < 0) {
            return -1;
        }
        p->inst[s].b = p->n;
        if (emit(p, n->b) != 0) {
            return -1;
        }
        p->inst[j].a = p->n;
        return 0;
    }
    case RX_REP:
        return emit_rep(p, n);
    case RX_GROUP: {
        int at = push(p, RX_OP_SAVE, 2 * n->cap, 0, 0);
        if (at < 0) {
            return -1;
        }
        if (emit(p, n->a) != 0) {
            return -1;
        }
        return push(p, RX_OP_SAVE, 2 * n->cap + 1, 0, 0) < 0 ? -1 : 0;
    }
    default:
        return -1;
    }
}

static void prog_free(struct rprog *p)
{
    if (p == NULL) {
        return;
    }
    free(p->inst);
    free(p->classes);
    free(p);
}

int regcomp(regex_t *preg, const char *regex, int cflags)
{
    struct rprog *p;
    struct rnode *ast;
    int err = 0;

    if (preg == NULL || regex == NULL) {
        return REG_BADPAT;
    }
    p = calloc(1, sizeof(*p));
    if (p == NULL) {
        return REG_ESPACE;
    }
    ast = rx_parse(regex, cflags, p, &err);
    if (ast == NULL) {
        prog_free(p);
        return err != 0 ? err : REG_BADPAT;
    }
    if (emit(p, ast) != 0 || push(p, RX_OP_MATCH, 0, 0, 0) < 0) {
        rx_ast_free(ast);
        prog_free(p);
        return REG_ESPACE;
    }
    rx_ast_free(ast);
    p->icase = (cflags & REG_ICASE) != 0;
    p->newline = (cflags & REG_NEWLINE) != 0;
    preg->re_nsub = (size_t)p->nsub;
    preg->__prog = p;
    preg->__cflags = cflags;
    return 0;
}

void regfree(regex_t *preg)
{
    if (preg == NULL) {
        return;
    }
    prog_free((struct rprog *)preg->__prog);
    preg->__prog = NULL;
    preg->re_nsub = 0;
}
