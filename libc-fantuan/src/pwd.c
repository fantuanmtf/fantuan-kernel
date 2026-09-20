/* libc-fantuan — pwd/grp (P1): one root account, no group database. */
#include <errno.h>
#include <pwd.h>
#include <string.h>

static struct passwd root_pw = {
    (char *)"root",
    (char *)"x",
    0,
    0,
    (char *)"root",
    (char *)"/",
    (char *)"/bin/sh",
};

struct passwd *getpwnam(const char *name)
{
    if (name && strcmp(name, "root") == 0) {
        return &root_pw;
    }
    errno = ENOENT;
    return NULL;
}

struct passwd *getpwuid(uid_t uid)
{
    if (uid == 0) {
        return &root_pw;
    }
    errno = ENOENT;
    return NULL;
}

void setpwent(void) {}
void endpwent(void) {}
struct passwd *getpwent(void) { return NULL; }

/* Group database: every lookup is the root group. */
struct group {
    char *gr_name;
    char *gr_passwd;
    unsigned int gr_gid;
    char **gr_mem;
};

static char *root_members[] = { (char *)"root", NULL };
static struct group root_gr = { (char *)"root", (char *)"x", 0, root_members };

struct group *getgrnam(const char *name)
{
    if (name && strcmp(name, "root") == 0) {
        return &root_gr;
    }
    errno = ENOENT;
    return NULL;
}

struct group *getgrgid(unsigned int gid)
{
    if (gid == 0) {
        return &root_gr;
    }
    errno = ENOENT;
    return NULL;
}

void setgrent(void) {}
void endgrent(void) {}
struct group *getgrent(void) { return NULL; }
