/* libc-fantuan — process layer (P2): fork/execve/wait4, process groups and
 * sessions over the native ABI (docs/POSIX_PLAN.md). */
#include <errno.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <fantuan/abi.h>

long __fantuan_raw(long n, long a1, long a2, long a3, long a4, long a5);

static long rc(long r)
{
    if (r < 0 && r > -4096) {
        errno = (int)-r;
        return -1;
    }
    return r;
}

pid_t fork(void)
{
    return (pid_t)rc(__fantuan_raw(FANTUAN_SYS_FORK, 0, 0, 0, 0, 0));
}

/* vfork: the eager-copy fork is safe to use in its place (no shared window,
 * which is the direction POSIX wants anyway). */
pid_t vfork(void)
{
    return fork();
}

int execve(const char *path, char *const argv[], char *const envp[])
{
    return (int)rc(__fantuan_raw(FANTUAN_SYS_EXECVE, (long)path, (long)argv,
                                 (long)envp, 0, 0));
}

int execv(const char *path, char *const argv[])
{
    extern char **environ;
    return execve(path, argv, environ);
}

/* PATH search; a name containing '/' is used verbatim. */
int execvp(const char *file, char *const argv[])
{
    if (strchr(file, '/') != NULL) {
        return execve(file, argv, NULL);
    }
    const char *path = getenv("PATH");
    if (path == NULL) {
        path = "/bin:/usr/bin";
    }
    char buf[256];
    while (*path != '\0') {
        const char *colon = strchr(path, ':');
        size_t len = colon ? (size_t)(colon - path) : strlen(path);
        if (len + 1 + strlen(file) + 1 <= sizeof(buf)) {
            if (len > 0) {
                memcpy(buf, path, len);
                buf[len] = '/';
                strcpy(buf + len + 1, file);
            } else {
                strcpy(buf, file);
            }
            execve(buf, argv, NULL);
            if (errno != ENOENT && errno != ENOTDIR) {
                return -1;
            }
        }
        if (!colon) {
            break;
        }
        path = colon + 1;
    }
    errno = ENOENT;
    return -1;
}

pid_t wait4(pid_t pid, int *status, int options, void *rusage)
{
    return (pid_t)rc(__fantuan_raw(FANTUAN_SYS_WAIT4, pid, (long)status,
                                   options, (long)rusage, 0));
}

pid_t waitpid(pid_t pid, int *status, int options)
{
    return wait4(pid, status, options, NULL);
}

pid_t wait(int *status)
{
    return wait4(-1, status, 0, NULL);
}

pid_t wait3(int *status, int options, void *rusage)
{
    return wait4(-1, status, options, rusage);
}

int setpgid(pid_t pid, pid_t pgid)
{
    return (int)rc(__fantuan_raw(FANTUAN_SYS_SETPGID, pid, pgid, 0, 0, 0));
}

pid_t getpgid(pid_t pid)
{
    return (pid_t)rc(__fantuan_raw(FANTUAN_SYS_GETPGID, pid, 0, 0, 0, 0));
}

pid_t getpgrp(void)
{
    return (pid_t)__fantuan_raw(FANTUAN_SYS_GETPGRP, 0, 0, 0, 0, 0);
}

pid_t setsid(void)
{
    return (pid_t)rc(__fantuan_raw(FANTUAN_SYS_SETSID, 0, 0, 0, 0, 0));
}
