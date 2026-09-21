/* libc-fantuan — pty.h (P3): forkpty/openpty are linkable stubs.
 * Fantuan's console is a single line-disciplined tty with no pty driver;
 * openpty/forkpty return -1/ENOSYS. bash 5.3 (built --disable-readline)
 * does not call them, so this is honest link surface, not a runtime path. */
#ifndef _PTY_H
#define _PTY_H

#include <sys/types.h>
#include <sys/ioctl.h>
#include <termios.h>

int openpty(int *amaster, int *aslave, char *name,
            const struct termios *termp, const struct winsize *winp);
pid_t forkpty(int *amaster, char *name,
              const struct termios *termp, const struct winsize *winp);

#endif /* _PTY_H */
