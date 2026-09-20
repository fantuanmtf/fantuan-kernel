/* libc-fantuan — strerror + BSD strings helpers (P1).
 *
 * Split from string.c to keep files within the repo's 300-line convention. */
#include <ctype.h>
#include <string.h>
#include <stdlib.h>

static const char *const errlist[] = {
    "Success",          /* 0 */
    "Not supported",    /* ENOSYS 1 */
    "Invalid argument", /* EINVAL 2 */
    "No such file or directory", /* ENOENT 3 */
    "Bad file descriptor",       /* EBADF 4 */
    "I/O error",                 /* EIO 5 */
    "Cannot allocate memory",    /* ENOMEM 6 */
    "Permission denied",         /* EACCES 7 */
    "File exists",               /* EEXIST 8 */
    "Not a directory",           /* ENOTDIR 9 */
    "Is a directory",            /* EISDIR 10 */
    "Directory not empty",       /* ENOTEMPTY 11 */
    "Numerical result out of range", /* ERANGE 12 */
    "Illegal seek",              /* ESPIPE 13 */
    "Inappropriate ioctl",       /* ENOTTY 14 */
    "Too many open files",       /* EMFILE 15 */
    "Bad address",               /* EFAULT 16 */
    "Resource temporarily unavailable", /* EAGAIN 17 */
    "Broken pipe",               /* EPIPE 18 */
    "Read-only file system",     /* EROFS 19 */
    "No such device",            /* ENODEV 20 */
    "File name too long",        /* ENAMETOOLONG 21 */
    "Too many levels of symbolic links", /* ELOOP 22 */
    "File too large",            /* EFBIG 23 */
    "No space left on device",   /* ENOSPC 24 */
    "Interrupted system call",   /* EINTR 25 */
    "No child processes",        /* ECHILD 26 */
    "Operation not permitted",   /* EPERM 27 */
    "No such process",           /* ESRCH 28 */
    "Argument list too long",    /* E2BIG 29 */
    "Exec format error",         /* ENOEXEC 30 */
    "Device or resource busy",   /* EBUSY 31 */
    "Too many open files in system", /* ENFILE 32 */
    "Too many links",            /* EMLINK 33 */
    "Numerical argument out of domain", /* EDOM 34 */
    "Value too large",           /* EOVERFLOW 35 */
    "Resource deadlock avoided", /* EDEADLK 36 */
};

char *strerror(int errnum)
{
    static char unknown[32];
    if (errnum >= 0 && (size_t)errnum < sizeof(errlist) / sizeof(errlist[0])) {
        return (char *)errlist[errnum];
    }
    strcpy(unknown, "Unknown error");
    return unknown;
}

int strcasecmp(const char *s1, const char *s2)
{
    while (*s1 && tolower((unsigned char)*s1) == tolower((unsigned char)*s2)) {
        s1++;
        s2++;
    }
    return tolower((unsigned char)*s1) - tolower((unsigned char)*s2);
}

int strncasecmp(const char *s1, const char *s2, size_t n)
{
    while (n && *s1 && tolower((unsigned char)*s1) == tolower((unsigned char)*s2)) {
        s1++;
        s2++;
        n--;
    }
    if (n == 0) {
        return 0;
    }
    return tolower((unsigned char)*s1) - tolower((unsigned char)*s2);
}

int ffs(int i)
{
    int bit = 1;
    if (i == 0) {
        return 0;
    }
    while (!(i & 1)) {
        i >>= 1;
        bit++;
    }
    return bit;
}

void bzero(void *s, size_t n)
{
    memset(s, 0, n);
}

void bcopy(const void *src, void *dest, size_t n)
{
    memmove(dest, src, n);
}

int bcmp(const void *s1, const void *s2, size_t n)
{
    return memcmp(s1, s2, n);
}

char *index(const char *s, int c)
{
    return strchr(s, c);
}

char *rindex(const char *s, int c)
{
    return strrchr(s, c);
}
