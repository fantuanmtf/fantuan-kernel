/* libc-fantuan — stdio file/character helpers (P1).
 *
 * Split from stdio.c to keep files within the repo's 300-line convention;
 * shares the FILE globals declared there. */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fantuan/abi.h>

int fputc(int c, FILE *stream)
{
    unsigned char b = (unsigned char)c;
    if (fwrite(&b, 1, 1, stream) != 1) {
        return EOF;
    }
    return b;
}

int putc(int c, FILE *stream)
{
    return fputc(c, stream);
}

int putchar(int c)
{
    return fputc(c, stdout);
}

int fputs(const char *s, FILE *stream)
{
    size_t n = strlen(s);
    return fwrite(s, 1, n, stream) == n ? 0 : EOF;
}

int puts(const char *s)
{
    if (fputs(s, stdout) == EOF) {
        return EOF;
    }
    return fputc('\n', stdout) == EOF ? EOF : 0;
}

int feof(FILE *stream) { return (stream->flags & _F_EOF) != 0; }
int ferror(FILE *stream) { return stream->error; }
void clearerr(FILE *stream) { stream->flags &= ~_F_EOF; stream->error = 0; }
int fileno(FILE *stream) { return stream->fd; }

static FILE *open_stream(const char *path, int fd, int flags)
{
    FILE *s = malloc(sizeof(FILE));
    if (!s) {
        if (fd >= 0) {
            close(fd);
        }
        return NULL;
    }
    memset(s, 0, sizeof(*s));
    s->fd = fd;
    s->flags = flags;
    (void)path;
    return s;
}

FILE *fopen(const char *path, const char *mode)
{
    int flags = O_RDONLY;
    int perms = _F_READ;
    if (mode[0] == 'w') {
        flags = O_WRONLY | O_CREAT | O_TRUNC;
        perms = _F_WRITE;
    } else if (mode[0] == 'a') {
        flags = O_WRONLY | O_CREAT | O_APPEND;
        perms = _F_WRITE;
    } else if (mode[0] == 'r') {
        flags = O_RDONLY;
        perms = _F_READ;
    }
    if (strchr(mode, '+')) {
        flags = O_RDWR;
        perms = _F_READ | _F_WRITE;
    }
    int fd = open(path, flags, 0644);
    if (fd < 0) {
        return NULL;
    }
    return open_stream(path, fd, perms);
}

FILE *fdopen(int fd, const char *mode)
{
    int perms = (mode && mode[0] == 'w') ? _F_WRITE : _F_READ | _F_WRITE;
    return open_stream(NULL, fd, perms);
}

FILE *freopen(const char *path, const char *mode, FILE *stream)
{
    FILE *n = fopen(path, mode);
    if (!n) {
        return NULL;
    }
    fclose(stream);
    return n;
}

int fclose(FILE *stream)
{
    int r = fflush(stream);
    if (stream->fd > 2) {
        if (close(stream->fd) < 0) {
            r = EOF;
        }
        free(stream);
    }
    return r;
}

void perror(const char *s)
{
    extern char *strerror(int);
    if (s && *s) {
        fprintf(stderr, "%s: %s\n", s, strerror(errno));
    } else {
        fprintf(stderr, "%s\n", strerror(errno));
    }
}

int remove(const char *path)
{
    return unlink(path);
}

int rename(const char *oldpath, const char *newpath)
{
    long r = __fantuan_syscall6(FANTUAN_SYS_RENAME, (long)oldpath, (long)newpath, 0, 0, 0);
    if (r < 0) {
        errno = (int)-r;
        return -1;
    }
    return 0;
}

int setvbuf(FILE *stream, char *buf, int mode, size_t size)
{
    (void)buf;
    (void)size;
    if (mode == _IONBF) {
        stream->flags &= ~_F_LINE;
    } else if (mode == _IOLBF) {
        stream->flags |= _F_LINE;
    }
    return 0;
}

void setbuf(FILE *stream, char *buf)
{
    setvbuf(stream, buf, buf ? _IOFBF : _IONBF, BUFSIZ);
}

FILE *tmpfile(void)
{
    errno = ENOSYS;
    return NULL;
}
