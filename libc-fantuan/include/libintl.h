/* libc-fantuan — libintl.h (P3): NLS is disabled, so gettext is the
 * identity function. The macros keep code that includes libintl.h directly
 * linkable; bash itself builds --disable-nls. */
#ifndef _LIBINTL_H
#define _LIBINTL_H

#ifdef __cplusplus
extern "C" {
#endif

char *gettext(const char *msgid);
char *dgettext(const char *domainname, const char *msgid);
char *dcgettext(const char *domainname, const char *msgid, int category);
char *ngettext(const char *msgid1, const char *msgid2, unsigned long n);
char *dngettext(const char *domainname, const char *msgid1, const char *msgid2,
                unsigned long n);
char *dcngettext(const char *domainname, const char *msgid1, const char *msgid2,
                 unsigned long n, int category);
char *textdomain(const char *domainname);
char *bindtextdomain(const char *domainname, const char *dirname);
char *bind_textdomain_codeset(const char *domainname, const char *codeset);

#ifdef __cplusplus
}
#endif

#endif /* _LIBINTL_H */
