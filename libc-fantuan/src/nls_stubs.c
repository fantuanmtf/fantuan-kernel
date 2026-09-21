/* libc-fantuan — NLS/dynamic-loading stubs (P3): iconv, dlfcn and libintl.
 * Everything is linkable; conversions and dynamic loading honestly fail
 * (EINVAL/ENOSYS), gettext is the identity. */
#include <dlfcn.h>
#include <errno.h>
#include <iconv.h>
#include <libintl.h>
#include <stddef.h>

/* --- iconv ---------------------------------------------------------------- */

iconv_t iconv_open(const char *tocode, const char *fromcode)
{
    (void)tocode;
    (void)fromcode;
    errno = EINVAL;
    return (iconv_t)-1;
}

size_t iconv(iconv_t cd, char **inbuf, size_t *inbytesleft,
             char **outbuf, size_t *outbytesleft)
{
    (void)cd;
    (void)inbuf;
    (void)inbytesleft;
    (void)outbuf;
    (void)outbytesleft;
    errno = EINVAL;
    return (size_t)-1;
}

int iconv_close(iconv_t cd)
{
    (void)cd;
    return 0;
}

/* --- dlfcn ---------------------------------------------------------------- */

void *dlopen(const char *filename, int flags)
{
    (void)filename;
    (void)flags;
    errno = ENOSYS;
    return NULL;
}

void *dlsym(void *handle, const char *symbol)
{
    (void)handle;
    (void)symbol;
    errno = ENOSYS;
    return NULL;
}

int dlclose(void *handle)
{
    (void)handle;
    return 0;
}

char *dlerror(void)
{
    return (char *)"dynamic loading is not supported by libc-fantuan";
}

/* --- libintl (NLS disabled) ----------------------------------------------- */

char *gettext(const char *msgid)
{
    return (char *)msgid;
}

char *dgettext(const char *domainname, const char *msgid)
{
    (void)domainname;
    return (char *)msgid;
}

char *dcgettext(const char *domainname, const char *msgid, int category)
{
    (void)domainname;
    (void)category;
    return (char *)msgid;
}

char *ngettext(const char *msgid1, const char *msgid2, unsigned long n)
{
    return (char *)(n == 1 ? msgid1 : msgid2);
}

char *dngettext(const char *domainname, const char *msgid1, const char *msgid2,
                unsigned long n)
{
    (void)domainname;
    return (char *)(n == 1 ? msgid1 : msgid2);
}

char *dcngettext(const char *domainname, const char *msgid1, const char *msgid2,
                 unsigned long n, int category)
{
    (void)domainname;
    (void)category;
    return (char *)(n == 1 ? msgid1 : msgid2);
}

char *textdomain(const char *domainname)
{
    (void)domainname;
    return (char *)"";
}

char *bindtextdomain(const char *domainname, const char *dirname)
{
    (void)domainname;
    (void)dirname;
    return (char *)"";
}

char *bind_textdomain_codeset(const char *domainname, const char *codeset)
{
    (void)domainname;
    (void)codeset;
    return (char *)"";
}
