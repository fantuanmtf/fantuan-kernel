/* libc-fantuan — pwd.h (P1): a single root account until a real user
 * database exists. */
#ifndef _PWD_H
#define _PWD_H

#include <sys/types.h>

struct passwd {
    char *pw_name;
    char *pw_passwd;
    uid_t pw_uid;
    gid_t pw_gid;
    char *pw_gecos;
    char *pw_dir;
    char *pw_shell;
};

struct passwd *getpwnam(const char *name);
struct passwd *getpwuid(uid_t uid);
void setpwent(void);
void endpwent(void);
struct passwd *getpwent(void);

#endif /* _PWD_H */
