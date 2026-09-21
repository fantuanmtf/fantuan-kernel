/* libc-fantuan — P1 stubs (present so configure-style probes link; P2 adds
 * fork/exec/wait, signals, select/poll and the resource accounting). */
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <poll.h>
#include <signal.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <sys/resource.h>
#include <sys/select.h>
#include <sys/stat.h>
#include <sys/times.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <fantuan/abi.h>

unsigned int alarm(unsigned int seconds)
{
    (void)seconds;
    return 0;
}

int pause(void)
{
    errno = ENOSYS;
    return -1;
}

int link(const char *oldpath, const char *newpath)
{
    (void)oldpath;
    (void)newpath;
    errno = ENOSYS;
    return -1;
}

int symlink(const char *target, const char *linkpath)
{
    (void)target;
    (void)linkpath;
    errno = ENOSYS;
    return -1;
}

ssize_t readlink(const char *path, char *buf, size_t bufsiz)
{
    (void)path;
    (void)buf;
    (void)bufsiz;
    errno = ENOSYS;
    return -1;
}

int chmod(const char *path, mode_t mode)
{
    (void)path;
    (void)mode;
    return 0; /* no permission model in P1 */
}

int select(int nfds, fd_set *readfds, fd_set *writefds, fd_set *exceptfds,
           struct timeval *timeout)
{
    (void)nfds;
    (void)readfds;
    (void)writefds;
    (void)exceptfds;
    if (timeout) {
        struct timespec ts = { timeout->tv_sec, timeout->tv_usec * 1000L };
        nanosleep(&ts, NULL);
        return 0;
    }
    errno = ENOSYS;
    return -1;
}

int poll(struct pollfd *fds, nfds_t nfds, int timeout)
{
    (void)fds;
    (void)nfds;
    if (timeout >= 0) {
        struct timespec ts = { timeout / 1000, (long)(timeout % 1000) * 1000000L };
        nanosleep(&ts, NULL);
        return 0;
    }
    errno = ENOSYS;
    return -1;
}

int getitimer(int which, void *curr_value)
{
    (void)which;
    (void)curr_value;
    errno = ENOSYS;
    return -1;
}

int setitimer(int which, const void *new_value, void *old_value)
{
    (void)which;
    (void)new_value;
    (void)old_value;
    errno = ENOSYS;
    return -1;
}

int getrusage(int who, struct rusage *usage)
{
    (void)who;
    if (usage) {
        memset(usage, 0, sizeof(*usage));
    }
    return 0;
}

clock_t times(struct tms *buf)
{
    if (buf) {
        memset(buf, 0, sizeof(*buf));
        buf->tms_utime = clock();
    }
    return clock();
}

int getrlimit(int resource, struct rlimit *rlim)
{
    if (!rlim) {
        errno = EINVAL;
        return -1;
    }
    rlim->rlim_cur = RLIM_INFINITY;
    rlim->rlim_max = RLIM_INFINITY;
    if (resource == RLIMIT_NOFILE) {
        rlim->rlim_cur = 16;
        rlim->rlim_max = 16;
    } else if (resource == RLIMIT_STACK) {
        rlim->rlim_cur = 16 * 1024;
        rlim->rlim_max = 16 * 1024;
    }
    return 0;
}

int setrlimit(int resource, const struct rlimit *rlim)
{
    (void)resource;
    (void)rlim;
    return 0; /* accepted, not enforced in P1 */
}

int getgroups(int size, gid_t list[])
{
    if (size > 0 && list) {
        list[0] = 0; /* root group */
    }
    return 1;
}

int getpriority(int which, int who)
{
    (void)which;
    (void)who;
    return 0;
}

int setpriority(int which, int who, int prio)
{
    (void)which;
    (void)who;
    (void)prio;
    return 0;
}

int flock(int fd, int operation)
{
    (void)fd;
    (void)operation;
    return 0; /* single-user P1: locks are accepted and ignored */
}

long sysconf(int name)
{
    switch (name) {
    case _SC_ARG_MAX: return 4096;
    case _SC_CHILD_MAX: return 0;
    case _SC_CLK_TCK: return 100;
    case _SC_NGROUPS_MAX: return 0;
    case _SC_OPEN_MAX: return 16;
    case _SC_JOB_CONTROL: return 0;
    case _SC_SAVED_IDS: return 1;
    case _SC_VERSION: return 200809L;
    case _SC_PAGESIZE: return 4096;
    default:
        errno = EINVAL;
        return -1;
    }
}

long pathconf(const char *path, int name)
{
    (void)path;
    if (name == _PC_NAME_MAX) {
        return 31;
    }
    if (name == _PC_PATH_MAX) {
        return 256;
    }
    errno = EINVAL;
    return -1;
}

int getpagesize(void)
{
    return 4096;
}

int scanf(const char *fmt, ...)
{
    (void)fmt;
    errno = ENOSYS;
    return EOF;
}

int sscanf(const char *s, const char *fmt, ...)
{
    (void)s;
    (void)fmt;
    errno = ENOSYS;
    return 0;
}

intmax_t strtoimax(const char *nptr, char **endptr, int base)
{
    return (intmax_t)strtoll(nptr, endptr, base);
}

uintmax_t strtoumax(const char *nptr, char **endptr, int base)
{
    return (uintmax_t)strtoull(nptr, endptr, base);
}
