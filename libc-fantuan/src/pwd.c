/* libc-fantuan — pwd/grp (P1): one root account, no group database. */
#include <errno.h>
#include <grp.h>
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

/* Group database: every lookup is the root group (struct group in grp.h). */
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

struct group *getgrgid(gid_t gid)
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

/* P3: bash's prompt/`complete` paths call this; the single root group is
 * always reported and *ngroups gets the true count on success. */
int getgrouplist(const char *user, gid_t group, gid_t *groups, int *ngroups)
{
    (void)user;
    if (ngroups == NULL) {
        errno = EINVAL;
        return -1;
    }
    if (groups == NULL || *ngroups < 1) {
        *ngroups = 1;
        return -1;
    }
    groups[0] = group;
    *ngroups = 1;
    return 1;
}
