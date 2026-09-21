/* libc-fantuan — regex parser (P3): recursive descent from a BRE/ERE
 * pattern to the compact AST declared in regex_priv.h.
 *
 * Original code written for libc-fantuan (MIT, not a port). Supported
 * syntax is listed in include/regex.h; unsupported constructs (e.g. BRE
 * back-references) are treated as ordinary characters. */
#include <ctype.h>
#include <stdlib.h>
#include <string.h>

#include "regex_priv.h"

struct px {
    const char *pat;
    int pos;
    int extended;
    int icase;
    int err;
    struct rprog *prog;
};

static int peek(struct px *x)
{
    return (unsigned char)x->pat[x->pos];
}

static int peek2(struct px *x)
{
    return x->pat[x->pos] ? (unsigned char)x->pat[x->pos + 1] : 0;
}

static int take(struct px *x)
{
    return x->pat[x->pos] ? (unsigned char)x->pat[x->pos++] : 0;
}

static int at_group_close(struct px *x)
{
    if (x->extended) {
        return peek(x) == ')';
    }
    return peek(x) == '\\' && peek2(x) == ')';
}

static int at_group_open(struct px *x)
{
    if (x->extended) {
        return peek(x) == '(';
    }
    return peek(x) == '\\' && peek2(x) == '(';
}

static int at_alt(struct px *x)
{
    if (x->extended) {
        return peek(x) == '|';
    }
    return peek(x) == '\\' && peek2(x) == '|';
}

static struct rnode *parse_alt(struct px *x);

static struct rnode *parse_atom(struct px *x)
{
    struct rnode *n;
    int c = peek(x);
    if (at_group_open(x)) {
        struct rnode *inner;
        x->pos += x->extended ? 1 : 2;
        inner = parse_alt(x);
        if (x->err) {
            rx_ast_free(inner);
            return NULL;
        }
        if (!at_group_close(x)) {
            rx_ast_free(inner);
            x->err = REG_EPAREN;
            return NULL;
        }
        x->pos += x->extended ? 1 : 2;
        if (x->prog->nsub >= RX_MAXSUB) {
            rx_ast_free(inner);
            x->err = REG_ESPACE;
            return NULL;
        }
        n = rx_new_node(RX_GROUP);
        if (n == NULL) {
            rx_ast_free(inner);
            x->err = REG_ESPACE;
            return NULL;
        }
        n->a = inner;
        n->cap = ++x->prog->nsub;
        return n;
    }
    if (c == '[') {
        int cls;
        const char *cp;
        take(x);
        cp = x->pat + x->pos;
        cls = rx_parse_class(&cp, x->icase, x->prog, &x->err);
        if (cls < 0) {
            return NULL;
        }
        x->pos = (int)(cp - x->pat);
        n = rx_new_node(RX_CLASS);
        if (n == NULL) {
            x->err = REG_ESPACE;
            return NULL;
        }
        n->cls = cls;
        return n;
    }
    if (c == '.') {
        take(x);
        return rx_new_node(RX_ANY);
    }
    if (c == '^') {
        take(x);
        return rx_new_node(RX_BOL);
    }
    if (c == '$') {
        take(x);
        return rx_new_node(RX_EOL);
    }
    if (c == '\\' && peek2(x) != 0) {
        take(x);
        c = take(x);
    } else {
        take(x);
    }
    n = rx_new_node(RX_CHAR);
    if (n == NULL) {
        x->err = REG_ESPACE;
        return NULL;
    }
    n->ch = x->icase ? tolower(c) : c;
    return n;
}

static struct rnode *parse_rep(struct px *x)
{
    struct rnode *a = parse_atom(x);
    if (a == NULL) {
        return NULL;
    }
    for (;;) {
        int c = peek(x);
        int min = 0, max = -1;
        int got = 0;
        if (c == '*') {
            take(x);
            got = 1;
        } else if (x->extended && c == '+') {
            take(x);
            min = 1;
            got = 1;
        } else if (x->extended && c == '?') {
            take(x);
            max = 1;
            got = 1;
        } else if (!x->extended && c == '\\' && (peek2(x) == '+' || peek2(x) == '?')) {
            int op = peek2(x);
            x->pos += 2;
            min = (op == '+') ? 1 : 0;
            max = (op == '?') ? 1 : -1;
            got = 1;
        } else if ((x->extended && c == '{') ||
                   (!x->extended && c == '\\' && peek2(x) == '{')) {
            const char *cp = x->pat + x->pos;
            int r = rx_parse_brace(&cp, x->extended, &min, &max);
            if (r < 0) {
                rx_ast_free(a);
                x->err = REG_BADBR;
                return NULL;
            }
            if (r > 0) {
                x->pos = (int)(cp - x->pat);
                got = 1;
            }
        }
        if (!got) {
            break;
        }
        {
            struct rnode *r = rx_new_node(RX_REP);
            if (r == NULL) {
                rx_ast_free(a);
                x->err = REG_ESPACE;
                return NULL;
            }
            r->a = a;
            r->min = min;
            r->max = max;
            a = r;
        }
    }
    return a;
}

static struct rnode *parse_cat(struct px *x)
{
    struct rnode *head = NULL;
    for (;;) {
        struct rnode *n;
        if (peek(x) == 0 || at_alt(x) || at_group_close(x)) {
            break;
        }
        n = parse_rep(x);
        if (x->err) {
            rx_ast_free(head);
            return NULL;
        }
        if (n == NULL) {
            break;
        }
        if (head == NULL) {
            head = n;
        } else {
            struct rnode *cat = rx_new_node(RX_CAT);
            if (cat == NULL) {
                rx_ast_free(head);
                rx_ast_free(n);
                x->err = REG_ESPACE;
                return NULL;
            }
            cat->a = head;
            cat->b = n;
            head = cat;
        }
    }
    return head;
}

static struct rnode *parse_alt(struct px *x)
{
    struct rnode *a = parse_cat(x);
    if (x->err) {
        return NULL;
    }
    while (at_alt(x)) {
        struct rnode *b;
        struct rnode *alt;
        x->pos += x->extended ? 1 : 2;
        b = parse_cat(x);
        if (x->err) {
            rx_ast_free(a);
            return NULL;
        }
        alt = rx_new_node(RX_ALT);
        if (alt == NULL) {
            rx_ast_free(a);
            rx_ast_free(b);
            x->err = REG_ESPACE;
            return NULL;
        }
        alt->a = a;
        alt->b = b;
        a = alt;
    }
    return a;
}

struct rnode *rx_parse(const char *pattern, int cflags, struct rprog *prog, int *err)
{
    struct px x;
    struct rnode *root;
    x.pat = pattern;
    x.pos = 0;
    x.extended = (cflags & REG_EXTENDED) != 0;
    x.icase = (cflags & REG_ICASE) != 0;
    x.err = 0;
    x.prog = prog;
    root = parse_alt(&x);
    if (x.err) {
        rx_ast_free(root);
        *err = x.err;
        return NULL;
    }
    if (peek(&x) != 0) {
        rx_ast_free(root);
        *err = REG_BADPAT;
        return NULL;
    }
    if (root == NULL) {
        root = rx_new_node(RX_EMPTY);
        if (root == NULL) {
            *err = REG_ESPACE;
            return NULL;
        }
    }
    *err = 0;
    return root;
}
