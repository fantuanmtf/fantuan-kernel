/* libc-fantuan — pty/forkpty stubs (P3): ENOSYS, documented in pty.h. */
#include <errno.h>
#include <pty.h>

int openpty(int *amaster, int *aslave, char *name,
            const struct termios *termp, const struct winsize *winp)
{
    (void)amaster;
    (void)aslave;
    (void)name;
    (void)termp;
    (void)winp;
    errno = ENOSYS;
    return -1;
}

pid_t forkpty(int *amaster, char *name,
              const struct termios *termp, const struct winsize *winp)
{
    (void)amaster;
    (void)name;
    (void)termp;
    (void)winp;
    errno = ENOSYS;
    return -1;
}
