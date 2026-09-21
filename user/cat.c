/* libc-fantuan — cat (P2): minimal concatenator for the dash userland.
 *
 * Usage: cat [file...]
 * With no arguments it copies stdin, so dash pipelines and redirections
 * ("echo hi | cat", "cat < /tmp/f") have a real /bin/cat to exec.
 * Diagnostics print argv[0] (the shipped binary has no hardcoded name:
 * the minimal-ELF string invariants in smoke-config stay clean). */
#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>

static int copy_fd(int fd)
{
    char buf[512];
    ssize_t n;
    while ((n = read(fd, buf, sizeof buf)) > 0) {
        ssize_t off = 0;
        while (off < n) {
            ssize_t w = write(1, buf + off, (size_t)(n - off));
            if (w <= 0) {
                return 1;
            }
            off += w;
        }
    }
    return n < 0 ? 1 : 0;
}

int main(int argc, char **argv)
{
    const char *name = (argc > 0 && argv[0]) ? argv[0] : "";
    if (argc < 2) {
        return copy_fd(0);
    }
    int rc = 0;
    for (int i = 1; i < argc; i++) {
        int fd = open(argv[i], O_RDONLY);
        if (fd < 0) {
            fprintf(stderr, "%s: %s: cannot open\n", name, argv[i]);
            rc = 1;
            continue;
        }
        rc |= copy_fd(fd);
        close(fd);
    }
    return rc;
}
