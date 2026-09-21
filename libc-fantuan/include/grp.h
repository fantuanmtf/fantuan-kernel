/* libc-fantuan — grp.h (P3): the group database is a single root group
 * (src/pwd.c); the header exists so bash and configure probes compile. */
#ifndef _GRP_H
#define _GRP_H

#include <sys/types.h>

struct group {
    char *gr_name;
    char *gr_passwd;
    gid_t gr_gid;
    char **gr_mem;
};

struct group *getgrnam(const char *name);
struct group *getgrgid(gid_t gid);
struct group *getgrent(void);
void setgrent(void);
void endgrent(void);
int getgrouplist(const char *user, gid_t group, gid_t *groups, int *ngroups);

#endif /* _GRP_H */
