/* proc_test.c — P2 process-layer evidence for fantuan (libc-fantuan).
 *
 * Proves fork returns twice, wait4 reaps with the right status, a pipe
 * survives fork, SIGCHLD/SIGINT handlers run, a blocked read is interrupted,
 * mmap-backed memory works, execve replaces the image and fds survive exec.
 * Markers are asserted by tools/smoke-posix.sh (P2 extension). */
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static volatile sig_atomic_t got_chld;
static volatile sig_atomic_t got_int;

static void on_chld(int sig)
{
    (void)sig;
    got_chld++;
}

static void on_int(int sig)
{
    (void)sig;
    got_int++;
}

static void nap_ms(long ms)
{
    struct timespec ts = { ms / 1000, (ms % 1000) * 1000000L };
    nanosleep(&ts, NULL);
}

int main(int argc, char **argv)
{
    if (argc > 1 && strcmp(argv[1], "--re-exec") == 0) {
        printf("user: proc_test re-exec image pid=%d\n", (int)getpid());
        _exit(7);
    }
    if (argc > 1 && strcmp(argv[1], "--pipe-child") == 0) {
        ssize_t w = write(1, "pipe-exec\n", 10);
        _exit(w == 10 ? 0 : 96);
    }

    printf("user: proc_test start pid=%d ppid=%d\n", (int)getpid(), (int)getppid());

    /* 1. fork returns twice; the child exits 42, the parent reaps it. */
    pid_t pid = fork();
    if (pid < 0) {
        printf("user: fork failed errno=%d\n", errno);
        return 1;
    }
    if (pid == 0) {
        _exit(42);
    }
    int status = 0;
    pid_t w = wait4(pid, &status, 0, NULL);
    printf("user: wait4 pid=%d exited=%d code=%d\n", (int)w, WIFEXITED(status),
           WEXITSTATUS(status));

    /* 2. pipe + fork: the child writes, the parent reads, wait4 reaps. */
    int fds[2];
    if (pipe(fds) != 0) {
        printf("user: pipe failed errno=%d\n", errno);
        return 1;
    }
    pid = fork();
    if (pid == 0) {
        close(fds[0]);
        ssize_t wr = write(fds[1], "pipex", 5);
        (void)wr;
        _exit(0);
    }
    close(fds[1]);
    char buf[32];
    memset(buf, 0, sizeof(buf));
    ssize_t pr = read(fds[0], buf, sizeof(buf) - 1);
    close(fds[0]);
    wait4(pid, &status, 0, NULL);
    printf("user: pipe-fork read=%d data=%s\n", (int)pr, buf);

    /* 3. SIGCHLD handler runs when the child exits. */
    signal(SIGCHLD, on_chld);
    pid = fork();
    if (pid == 0) {
        _exit(0);
    }
    for (int i = 0; i < 200 && !got_chld; i++) {
        nap_ms(10);
    }
    wait4(pid, &status, 0, NULL);
    printf("user: sigchld handler=%d\n", (int)got_chld);

    /* 4. SIGINT handler runs on raise(). */
    signal(SIGINT, on_int);
    raise(SIGINT);
    printf("user: sigint handler=%d\n", (int)got_int);
    signal(SIGINT, SIG_DFL);

    /* 5. mmap-backed anonymous memory (MAP_PRIVATE|MAP_ANONYMOUS). */
    char *m = mmap(NULL, 65536, PROT_READ | PROT_WRITE,
                   MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (m != MAP_FAILED) {
        memset(m, 'A', 65536);
        printf("user: mmap ok first=%c last=%c\n", m[0], m[65535]);
        if (munmap(m, 65536) != 0) {
            printf("user: munmap failed errno=%d\n", errno);
        }
    } else {
        printf("user: mmap failed errno=%d\n", errno);
    }

    /* 6. execve replaces the image (child runs proc_test --re-exec). */
    pid = fork();
    if (pid == 0) {
        char *av[] = { (char *)"/bin/proc-test", (char *)"--re-exec", NULL };
        char *ev[] = { (char *)"HOME=/", NULL };
        execve("/bin/proc-test", av, ev);
        _exit(99);
    }
    wait4(pid, &status, 0, NULL);
    printf("user: exec child exited=%d code=%d\n", WIFEXITED(status),
           WEXITSTATUS(status));

    /* 7. fds survive exec: the child execs hello with stdout on a pipe. */
    if (pipe(fds) != 0) {
        return 1;
    }
    pid = fork();
    if (pid == 0) {
        close(fds[0]);
        if (dup2(fds[1], 1) != 1) {
            write(2, "user: pipe-child dup2 failed\n", 29);
            _exit(97);
        }
        close(fds[1]);
        char *av[] = { (char *)"/bin/proc-test", (char *)"--pipe-child", NULL };
        execve("/bin/proc-test", av, NULL);
        _exit(98);
    }
    close(fds[1]);
    int total = 0;
    int saw_hello = 0;
    for (;;) {
        char chunk[128];
        ssize_t n = read(fds[0], chunk, sizeof(chunk));
        if (n <= 0) {
            break;
        }
        total += (int)n;
        chunk[n < (ssize_t)sizeof(chunk) ? n : (ssize_t)sizeof(chunk) - 1] = 0;
        if (strstr(chunk, "hello from C") != NULL) {
            saw_hello = 1;
        }
    }
    close(fds[0]);
    wait4(pid, &status, 0, NULL);
    printf("user: exec-pipe bytes=%d hello=%d status=%d exit=%d sig=%d\n", total,
           saw_hello, status, WIFEXITED(status) ? WEXITSTATUS(status) : -1,
           WIFSIGNALED(status) ? WTERMSIG(status) : 0);

    /* 8. a blocked read is interrupted by SIGINT and returns EINTR. */
    got_int = 0;
    signal(SIGINT, on_int);
    if (pipe(fds) != 0) {
        return 1;
    }
    pid = fork();
    if (pid == 0) {
        close(fds[0]);
        printf("user: intr child ppid=%d self=%d\n", (int)getppid(), (int)getpid());
        nap_ms(50);
        int kr = kill(getppid(), SIGINT);
        printf("user: intr child kill=%d\n", kr);
        nap_ms(300); /* keep the write end open while the parent is blocked */
        _exit(0);
    }
    close(fds[1]);
    errno = 0;
    ssize_t ir = read(fds[0], buf, 1);
    close(fds[0]);
    wait4(pid, &status, 0, NULL);
    printf("user: intr read=%d errno=%d handler=%d\n", (int)ir, errno,
           (int)got_int);

    printf("user: proc_test exit code 0\n");
    return 0;
}
