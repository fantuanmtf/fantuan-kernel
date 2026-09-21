/* libc-fantuan — popen/pclose (P3): pipe + fork + /bin/sh -c, the
 * POSIX.1-2008 direction (the child runs the system shell). */
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

#define POPEN_MAX 8

static struct {
    FILE *fp;
    pid_t pid;
} table[POPEN_MAX];

FILE *popen(const char *command, const char *mode)
{
    int fds[2];
    pid_t pid;
    int reading;
    int slot;
    FILE *fp;

    if (command == NULL || mode == NULL || (mode[0] != 'r' && mode[0] != 'w')) {
        errno = EINVAL;
        return NULL;
    }
    reading = (mode[0] == 'r');
    if (pipe(fds) != 0) {
        return NULL;
    }
    pid = fork();
    if (pid < 0) {
        close(fds[0]);
        close(fds[1]);
        return NULL;
    }
    if (pid == 0) {
        extern char **environ;
        char *argv[4];
        if (reading) {
            dup2(fds[1], STDOUT_FILENO);
        } else {
            dup2(fds[0], STDIN_FILENO);
        }
        close(fds[0]);
        close(fds[1]);
        argv[0] = (char *)"sh";
        argv[1] = (char *)"-c";
        argv[2] = (char *)command;
        argv[3] = NULL;
        execve("/bin/sh", argv, environ);
        _exit(127);
    }
    close(reading ? fds[1] : fds[0]);
    fp = fdopen(reading ? fds[0] : fds[1], reading ? "r" : "w");
    if (fp == NULL) {
        close(reading ? fds[0] : fds[1]);
        waitpid(pid, NULL, 0);
        return NULL;
    }
    for (slot = 0; slot < POPEN_MAX; slot++) {
        if (table[slot].fp == NULL) {
            table[slot].fp = fp;
            table[slot].pid = pid;
            return fp;
        }
    }
    fclose(fp);
    waitpid(pid, NULL, 0);
    errno = EMFILE;
    return NULL;
}

int pclose(FILE *stream)
{
    int slot;
    int status = -1;
    for (slot = 0; slot < POPEN_MAX; slot++) {
        if (table[slot].fp == stream) {
            pid_t pid = table[slot].pid;
            table[slot].fp = NULL;
            table[slot].pid = 0;
            fclose(stream);
            if (waitpid(pid, &status, 0) < 0) {
                return -1;
            }
            return status;
        }
    }
    errno = ECHILD;
    return -1;
}
