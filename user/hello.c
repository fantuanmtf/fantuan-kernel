/* hello.c — P1 first C user program for fantuan (libc-fantuan).
 *
 * Exercises the pipeline the next batches depend on: SysV argv from the
 * kernel, printf to fd 1 (/dev/console), brk-backed malloc, tmpfs
 * open/write/read/stat, getcwd and clock_gettime, then exit. Markers are
 * asserted by tools/smoke-posix.sh. */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

int main(int argc, char **argv)
{
    const char *arg0 = (argc > 0 && argv[0]) ? argv[0] : "(none)";
    printf("user: hello from C (argc=%d argv0=%s)\n", argc, arg0);
    printf("user: pid %d\n", (int)getpid());

    /* malloc over brk: force a few allocations and a free/realloc cycle. */
    char *heap = malloc(256);
    if (heap == NULL) {
        printf("user: malloc failed (errno=%d)\n", errno);
        return 3;
    }
    strcpy(heap, "heap ok");
    char *big = calloc(128, 1);
    char *again = realloc(big, 4096);
    free(heap);
    printf("user: malloc %s (realloc=%s)\n", again ? "brk" : "?", again ? "ok" : "fail");

    char cwd[64];
    if (getcwd(cwd, sizeof(cwd)) != NULL) {
        printf("user: cwd %s\n", cwd);
    } else {
        printf("user: getcwd failed (errno=%d)\n", errno);
    }

    /* tmpfs round trip through the fd syscalls. */
    const char *path = "/tmp/hello.txt";
    const char *msg = "libc-fantuan tmpfs round trip";
    int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) {
        printf("user: open %s failed (errno=%d)\n", path, errno);
        return 2;
    }
    ssize_t w = write(fd, msg, strlen(msg));
    close(fd);

    char buf[64];
    memset(buf, 0, sizeof(buf));
    fd = open(path, O_RDONLY, 0);
    if (fd < 0) {
        printf("user: reopen %s failed (errno=%d)\n", path, errno);
        return 2;
    }
    ssize_t r = read(fd, buf, sizeof(buf) - 1);
    close(fd);

    struct stat st;
    if (stat(path, &st) == 0) {
        printf("user: tmpfs %s size=%ld write=%ld read=%ld match=%d\n", path,
               (long)st.st_size, (long)w, (long)r,
               (r > 0 && strcmp(buf, msg) == 0) ? 1 : 0);
    } else {
        printf("user: stat %s failed (errno=%d)\n", path, errno);
    }

    /* A pipe written and read back in one task (fork arrives in P2). */
    int fds[2];
    if (pipe(fds) == 0) {
        ssize_t pw = write(fds[1], "pipe", 4);
        char pbuf[8];
        memset(pbuf, 0, sizeof(pbuf));
        ssize_t pr = read(fds[0], pbuf, sizeof(pbuf) - 1);
        close(fds[0]);
        close(fds[1]);
        printf("user: pipe write=%ld read=%ld data=%s\n", (long)pw, (long)pr,
               pr > 0 ? pbuf : "?");
    } else {
        printf("user: pipe failed (errno=%d)\n", errno);
    }

    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) == 0) {
        printf("user: uptime %ld.%09ld s\n", (long)ts.tv_sec, (long)ts.tv_nsec);
    }

    printf("user: exit code 0\n");
    return 0;
}
