/* libc-fantuan — paths.h (P1): the P1 root filesystem is the tmpfs from
 * docs/POSIX_PLAN.md; the built-in shell and /mnt disk mounts cover rescue. */
#ifndef _PATHS_H
#define _PATHS_H

#define _PATH_BSHELL "/bin/sh"
#define _PATH_CONSOLE "/dev/console"
#define _PATH_DEVNULL "/dev/null"
#define _PATH_TTY "/dev/console"
#define _PATH_TMP "/tmp/"
#define _PATH_VARTMP "/tmp/"
#define _PATH_DEFPATH "/bin:/usr/bin"

#endif /* _PATHS_H */
