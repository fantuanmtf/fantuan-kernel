/* libc-fantuan — syscall wrappers (P1).
 *
 * A syscall returns -errno (Fantuan numbering, see errno.h); every wrapper
 * converts that into the -1 + errno convention. Raw calls are for libc
 * internals that need the bare result. */
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdlib.h>
#include <sys/ioctl.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>
#include <fantuan/abi.h>

static long rc(long r)
{
    if (r < 0 && r > -4096) {
        errno = (int)-r;
        return -1;
    }
    return r;
}

long __fantuan_raw(long n, long a1, long a2, long a3, long a4, long a5)
{
    return __fantuan_syscall6(n, a1, a2, a3, a4, a5);
}

int open(const char *path, int flags, ...)
{
    va_list ap;
    long mode = 0;
    if (flags & O_CREAT) {
        va_start(ap, flags);
        mode = va_arg(ap, long);
        va_end(ap);
    }
    return (int)rc(__fantuan_raw(FANTUAN_SYS_OPEN, (long)path, flags, mode, 0, 0));
}

int creat(const char *path, mode_t mode)
{
    return open(path, O_WRONLY | O_CREAT | O_TRUNC, mode);
}

int close(int fd)
{
    return (int)rc(__fantuan_raw(FANTUAN_SYS_CLOSE, fd, 0, 0, 0, 0));
}

ssize_t read(int fd, void *buf, size_t count)
{
    return (ssize_t)rc(__fantuan_raw(FANTUAN_SYS_READ, fd, (long)buf, (long)count, 0, 0));
}

ssize_t write(int fd, const void *buf, size_t count)
{
    return (ssize_t)rc(__fantuan_raw(FANTUAN_SYS_WRITE_FD, fd, (long)buf, (long)count, 0, 0));
}

off_t lseek(int fd, off_t offset, int whence)
{
    return (off_t)rc(__fantuan_raw(FANTUAN_SYS_LSEEK, fd, offset, whence, 0, 0));
}

int pipe(int fds[2])
{
    return (int)rc(__fantuan_raw(FANTUAN_SYS_PIPE, (long)fds, 0, 0, 0, 0));
}

int dup(int fd)
{
    return (int)rc(__fantuan_raw(FANTUAN_SYS_DUP, fd, 0, 0, 0, 0));
}

int dup2(int oldfd, int newfd)
{
    return (int)rc(__fantuan_raw(FANTUAN_SYS_DUP2, oldfd, newfd, 0, 0, 0));
}

int chdir(const char *path)
{
    return (int)rc(__fantuan_raw(FANTUAN_SYS_CHDIR, (long)path, 0, 0, 0, 0));
}

char *getcwd(char *buf, size_t size)
{
    long r = rc(__fantuan_raw(FANTUAN_SYS_GETCWD, (long)buf, (long)size, 0, 0, 0));
    return r < 0 ? NULL : buf;
}

int unlink(const char *path)
{
    return (int)rc(__fantuan_raw(FANTUAN_SYS_UNLINK, (long)path, 0, 0, 0, 0));
}

int rmdir(const char *path)
{
    return (int)rc(__fantuan_raw(FANTUAN_SYS_RMDIR, (long)path, 0, 0, 0, 0));
}

int mkdir(const char *path, mode_t mode)
{
    return (int)rc(__fantuan_raw(FANTUAN_SYS_MKDIR, (long)path, (long)mode, 0, 0, 0));
}

int stat(const char *path, struct stat *buf)
{
    return (int)rc(__fantuan_raw(FANTUAN_SYS_STAT, (long)path, (long)buf, 0, 0, 0));
}

int lstat(const char *path, struct stat *buf)
{
    return stat(path, buf); /* no symlinks in P1 */
}

int fstat(int fd, struct stat *buf)
{
    return (int)rc(__fantuan_raw(FANTUAN_SYS_FSTAT, fd, (long)buf, 0, 0, 0));
}

int ioctl(int fd, unsigned long request, ...)
{
    va_list ap;
    void *arg;
    va_start(ap, request);
    arg = va_arg(ap, void *);
    va_end(ap);
    return (int)rc(__fantuan_raw(FANTUAN_SYS_IOCTL, fd, (long)request, (long)arg, 0, 0));
}

pid_t getpid(void)
{
    return (pid_t)__fantuan_raw(FANTUAN_SYS_GETPID, 0, 0, 0, 0, 0);
}

pid_t getppid(void)
{
    return (pid_t)__fantuan_raw(FANTUAN_SYS_GETPPID, 0, 0, 0, 0, 0);
}

uid_t getuid(void) { return 0; }
uid_t geteuid(void) { return 0; }
gid_t getgid(void) { return 0; }
gid_t getegid(void) { return 0; }

int isatty(int fd)
{
    struct termios t;
    return ioctl(fd, TCGETS, &t) == 0;
}

char *ttyname(int fd)
{
    return isatty(fd) ? (char *)"/dev/console" : NULL;
}

mode_t umask(mode_t mask)
{
    (void)mask;
    return 0;
}

int access(const char *path, int mode)
{
    struct stat st;
    (void)mode; /* P1 has no permission model */
    return stat(path, &st);
}

int chown(const char *path, uid_t owner, gid_t group)
{
    (void)path; (void)owner; (void)group;
    return 0;
}

int fchown(int fd, uid_t owner, gid_t group)
{
    (void)fd; (void)owner; (void)group;
    return 0;
}

int fcntl(int fd, int cmd, ...)
{
    (void)fd;
    switch (cmd) {
    case F_GETFD:
    case F_SETFD:
        return 0;
    default:
        errno = ENOSYS;
        return -1;
    }
}
