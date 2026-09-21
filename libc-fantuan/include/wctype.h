/* libc-fantuan — wctype.h (P3): ASCII classification over wint_t.
 * wchar.h carries wint_t/wctype_t so both headers agree. */
#ifndef _WCTYPE_H
#define _WCTYPE_H

#include <wchar.h>

#define WCTYPE_ALNUM 1
#define WCTYPE_ALPHA 2
#define WCTYPE_BLANK 3
#define WCTYPE_CNTRL 4
#define WCTYPE_DIGIT 5
#define WCTYPE_GRAPH 6
#define WCTYPE_LOWER 7
#define WCTYPE_PRINT 8
#define WCTYPE_PUNCT 9
#define WCTYPE_SPACE 10
#define WCTYPE_UPPER 11
#define WCTYPE_XDIGIT 12

int iswalnum(wint_t wc);
int iswalpha(wint_t wc);
int iswblank(wint_t wc);
int iswcntrl(wint_t wc);
int iswdigit(wint_t wc);
int iswgraph(wint_t wc);
int iswlower(wint_t wc);
int iswprint(wint_t wc);
int iswpunct(wint_t wc);
int iswspace(wint_t wc);
int iswupper(wint_t wc);
int iswxdigit(wint_t wc);
int iswctype(wint_t wc, wctype_t type);
wctype_t wctype(const char *property);
wint_t towlower(wint_t wc);
wint_t towupper(wint_t wc);

#endif /* _WCTYPE_H */
