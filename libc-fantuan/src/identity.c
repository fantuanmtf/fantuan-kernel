/* libc-fantuan — identity stubs (P3): the single root account has no
 * hostname syscall, no group changes and no uid switching. Split from
 * stubs.c to stay inside the repo's 300-line convention. */
#include <errno.h>
#include <string.h>
#include <unistd.h>

/* The host is always "fantuan"; there is no DNS or hostname syscall yet. */
int gethostname(char *name, size_t len)
{
    if (name == NULL || len == 0) {
        errno = EINVAL;
        return -1;
    }
    if (len < 8) {
        errno = ENAMETOOLONG;
        return -1;
    }
    strcpy(name, "fantuan");
    return 0;
}

int sethostname(const char *name, size_t len)
{
    (void)name;
    (void)len;
    errno = EPERM;
    return -1;
}

/* P3: single root identity. Dropping to root or staying put succeeds;
 * changing to any other uid/gid is honestly refused. */
int setuid(uid_t uid)
{
    if (uid == 0) {
        return 0;
    }
    errno = EPERM;
    return -1;
}

int setgid(gid_t gid)
{
    if (gid == 0) {
        return 0;
    }
    errno = EPERM;
    return -1;
}

int seteuid(uid_t uid)
{
    return setuid(uid);
}

int setegid(gid_t gid)
{
    return setgid(gid);
}

int setreuid(uid_t ruid, uid_t euid)
{
    return (ruid == 0 || ruid == (uid_t)-1) && (euid == 0 || euid == (uid_t)-1)
               ? 0
               : (errno = EPERM, -1);
}

int setregid(gid_t rgid, gid_t egid)
{
    return (rgid == 0 || rgid == (gid_t)-1) && (egid == 0 || egid == (gid_t)-1)
               ? 0
               : (errno = EPERM, -1);
}
