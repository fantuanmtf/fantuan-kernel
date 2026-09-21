/* libc-fantuan — regex VM/regexec (P3): depth-first backtracking over the
 * bytecode emitted by regex_compile.c; original code written for
 * libc-fantuan (MIT, not a port). A step budget bounds pathological
 * backtracking, so a failed match always returns. */
#include <ctype.h>
#include <stdlib.h>
#include <string.h>

#include "regex_priv.h"

#define RX_CAPSLOTS (2 * (RX_MAXSUB + 1))

struct vm {
    const struct rprog *p;
    const char *s;
    size_t len;
    int notbol;
    int noteol;
    long steps;
    size_t end;
    regoff_t cap[RX_CAPSLOTS];
};

int rx_class_match(const struct rprog *p, int cls, int c)
{
    unsigned char u = (unsigned char)c;
    return (p->classes[(size_t)cls * RX_CLASS_BYTES + (u >> 3)] >> (u & 7)) & 1;
}

static int chr_eq(const struct vm *v, int pattern_ch, int text_ch)
{
    if (v->p->icase) {
        return tolower(pattern_ch & 0xFF) == tolower(text_ch & 0xFF);
    }
    return (pattern_ch & 0xFF) == (text_ch & 0xFF);
}

static int run(struct vm *v, int pc, size_t sp)
{
    while (v->steps-- > 0) {
        const struct rinst *in = &v->p->inst[pc];
        switch (in->op) {
        case RX_OP_CHAR:
            if (sp < v->len && chr_eq(v, in->a, (unsigned char)v->s[sp])) {
                pc++;
                sp++;
                continue;
            }
            return 0;
        case RX_OP_ANY:
            if (sp < v->len && !(v->p->newline && v->s[sp] == '\n')) {
                pc++;
                sp++;
                continue;
            }
            return 0;
        case RX_OP_CLASS:
            if (sp < v->len && rx_class_match(v->p, in->a, (unsigned char)v->s[sp])) {
                pc++;
                sp++;
                continue;
            }
            return 0;
        case RX_OP_BOL:
            if (sp == 0 ? !v->notbol : (v->p->newline && v->s[sp - 1] == '\n')) {
                pc++;
                continue;
            }
            return 0;
        case RX_OP_EOL:
            if (sp == v->len ? !v->noteol : (v->p->newline && v->s[sp] == '\n')) {
                pc++;
                continue;
            }
            return 0;
        case RX_OP_SAVE: {
            regoff_t old = v->cap[in->a];
            v->cap[in->a] = (regoff_t)sp;
            if (run(v, pc + 1, sp)) {
                return 1;
            }
            v->cap[in->a] = old;
            return 0;
        }
        case RX_OP_SPLIT:
            if (run(v, in->a, sp)) {
                return 1;
            }
            pc = in->b;
            continue;
        case RX_OP_JMP:
            pc = in->a;
            continue;
        case RX_OP_MATCH:
            v->end = sp;
            return 1;
        default:
            return 0;
        }
    }
    return 0;
}

int regexec(const regex_t *preg, const char *string, size_t nmatch,
            regmatch_t pmatch[], int eflags)
{
    const struct rprog *p;
    struct vm v;
    size_t start;

    if (preg == NULL || string == NULL) {
        return REG_BADPAT;
    }
    p = (const struct rprog *)preg->__prog;
    if (p == NULL) {
        return REG_BADPAT;
    }
    memset(&v, 0, sizeof(v));
    v.p = p;
    v.s = string;
    v.len = strlen(string);
    v.notbol = (eflags & REG_NOTBOL) != 0;
    v.noteol = (eflags & REG_NOTEOL) != 0;

    for (start = 0; start <= v.len; start++) {
        size_t i;
        v.steps = 200000L + (long)v.len * 400L;
        for (i = 0; i < RX_CAPSLOTS; i++) {
            v.cap[i] = -1;
        }
        v.cap[0] = (regoff_t)start;
        if (run(&v, 0, start)) {
            v.cap[1] = (regoff_t)v.end;
            if (nmatch > 0 && pmatch != NULL) {
                for (i = 0; i < nmatch; i++) {
                    if (i <= (size_t)p->nsub && v.cap[2 * i] >= 0) {
                        pmatch[i].rm_so = v.cap[2 * i];
                        pmatch[i].rm_eo = v.cap[2 * i + 1] >= 0 ? v.cap[2 * i + 1] : v.cap[1];
                    } else {
                        pmatch[i].rm_so = -1;
                        pmatch[i].rm_eo = -1;
                    }
                }
            }
            return 0;
        }
    }
    return REG_NOMATCH;
}

size_t regerror(int errcode, const regex_t *preg, char *errbuf, size_t errbuf_size)
{
    static const char *const msgs[] = {
        "success",
        "no match",
        "invalid regular expression",
        "invalid collating element",
        "invalid character class",
        "trailing backslash",
        "invalid back reference",
        "unmatched [ or [^",
        "unmatched ( or \\(",
        "unmatched \\{",
        "invalid content of \\{\\}",
        "invalid range end",
        "out of memory",
        "invalid preceding regular expression",
        "unsupported",
    };
    const char *msg;
    size_t n;

    (void)preg;
    if (errcode < 0 || (size_t)errcode >= sizeof(msgs) / sizeof(msgs[0])) {
        msg = "unknown regex error";
    } else {
        msg = msgs[errcode];
    }
    n = strlen(msg) + 1;
    if (errbuf != NULL && errbuf_size > 0) {
        strncpy(errbuf, msg, errbuf_size - 1);
        errbuf[errbuf_size - 1] = '\0';
    }
    return n;
}
