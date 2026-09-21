/* libc-fantuan — wide classification (P3): ASCII semantics for the C
 * locale; wctype() maps the POSIX class names to the public constants. */
#include <ctype.h>
#include <string.h>
#include <wctype.h>

int iswalnum(wint_t wc)
{
    return wc <= 0xFF && isalnum((int)wc);
}

int iswalpha(wint_t wc)
{
    return wc <= 0xFF && isalpha((int)wc);
}

int iswblank(wint_t wc)
{
    return wc == ' ' || wc == '\t';
}

int iswcntrl(wint_t wc)
{
    return wc <= 0xFF && iscntrl((int)wc);
}

int iswdigit(wint_t wc)
{
    return wc <= 0xFF && isdigit((int)wc);
}

int iswgraph(wint_t wc)
{
    return wc <= 0xFF && isgraph((int)wc);
}

int iswlower(wint_t wc)
{
    return wc <= 0xFF && islower((int)wc);
}

int iswprint(wint_t wc)
{
    return wc <= 0xFF && isprint((int)wc);
}

int iswpunct(wint_t wc)
{
    return wc <= 0xFF && ispunct((int)wc);
}

int iswspace(wint_t wc)
{
    return wc <= 0xFF && isspace((int)wc);
}

int iswupper(wint_t wc)
{
    return wc <= 0xFF && isupper((int)wc);
}

int iswxdigit(wint_t wc)
{
    return wc <= 0xFF && isxdigit((int)wc);
}

wint_t towlower(wint_t wc)
{
    return wc <= 0xFF ? (wint_t)tolower((int)wc) : wc;
}

wint_t towupper(wint_t wc)
{
    return wc <= 0xFF ? (wint_t)toupper((int)wc) : wc;
}

wctype_t wctype(const char *property)
{
    if (property == NULL) {
        return 0;
    }
    if (strcmp(property, "alnum") == 0) return WCTYPE_ALNUM;
    if (strcmp(property, "alpha") == 0) return WCTYPE_ALPHA;
    if (strcmp(property, "blank") == 0) return WCTYPE_BLANK;
    if (strcmp(property, "cntrl") == 0) return WCTYPE_CNTRL;
    if (strcmp(property, "digit") == 0) return WCTYPE_DIGIT;
    if (strcmp(property, "graph") == 0) return WCTYPE_GRAPH;
    if (strcmp(property, "lower") == 0) return WCTYPE_LOWER;
    if (strcmp(property, "print") == 0) return WCTYPE_PRINT;
    if (strcmp(property, "punct") == 0) return WCTYPE_PUNCT;
    if (strcmp(property, "space") == 0) return WCTYPE_SPACE;
    if (strcmp(property, "upper") == 0) return WCTYPE_UPPER;
    if (strcmp(property, "xdigit") == 0) return WCTYPE_XDIGIT;
    return 0;
}

int iswctype(wint_t wc, wctype_t type)
{
    switch (type) {
    case WCTYPE_ALNUM: return iswalnum(wc);
    case WCTYPE_ALPHA: return iswalpha(wc);
    case WCTYPE_BLANK: return iswblank(wc);
    case WCTYPE_CNTRL: return iswcntrl(wc);
    case WCTYPE_DIGIT: return iswdigit(wc);
    case WCTYPE_GRAPH: return iswgraph(wc);
    case WCTYPE_LOWER: return iswlower(wc);
    case WCTYPE_PRINT: return iswprint(wc);
    case WCTYPE_PUNCT: return iswpunct(wc);
    case WCTYPE_SPACE: return iswspace(wc);
    case WCTYPE_UPPER: return iswupper(wc);
    case WCTYPE_XDIGIT: return iswxdigit(wc);
    default: return 0;
    }
}
