/* libc-fantuan — buffered stdio over the fd syscalls (P1).
 *
 * stdout is line buffered, stderr unbuffered, stdin fully buffered. The
 * printf core lives in printf.c; this file owns FILE, open/close/read/write
 * and the line helpers. No locking (single-threaded until P2/P3). */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include <fantuan/abi.h>

#define _F_READ 1
#define _F_WRITE 2
#define _F_LINE 4
#define _F_EOF 8

static FILE stdin_s = { 0, _F_READ, { 0 }, 0, 0, 0, 0 };
static FILE stdout_s = { 1, _F_WRITE | _F_LINE, { 0 }, 0, 0, 0, 0 };
static FILE stderr_s = { 2, _F_WRITE, { 0 }, 0, 0, 0, 0 };

FILE *stdin = &stdin_s;
FILE *stdout = &stdout_s;
FILE *stderr = &stderr_s;

void __stdio_flush_all(void)
{
    fflush(stdout);
    fflush(stderr);
}

int fflush(FILE *stream)
{
    if (stream == NULL) {
        return __stdio_flush_all(), 0;
    }
    if (stream->wlen > 0) {
        size_t off = 0;
        while (off < stream->wlen) {
            ssize_t n = write(stream->fd, stream->buf + off, stream->wlen - off);
            if (n <= 0) {
                stream->error = 1;
                stream->wlen = 0;
                return EOF;
            }
            off += (size_t)n;
        }
        stream->wlen = 0;
    }
    return 0;
}

static void put_buffered(FILE *stream, const unsigned char *p, size_t n)
{
    if (n >= BUFSIZ) {
        fflush(stream);
        size_t off = 0;
        while (off < n) {
            ssize_t w = write(stream->fd, p + off, n - off);
            if (w <= 0) {
                stream->error = 1;
                return;
            }
            off += (size_t)w;
        }
        return;
    }
    if (stream->wlen + n > BUFSIZ) {
        fflush(stream);
    }
    memcpy(stream->buf + stream->wlen, p, n);
    stream->wlen += n;
    if ((stream->flags & _F_LINE) && memchr(p, '\n', n)) {
        fflush(stream);
    }
}

size_t fwrite(const void *ptr, size_t size, size_t nmemb, FILE *stream)
{
    if (size == 0 || nmemb == 0) {
        return 0;
    }
    if (!(stream->flags & _F_WRITE)) {
        stream->error = 1;
        return 0;
    }
    size_t total = size * nmemb;
    put_buffered(stream, ptr, total);
    return stream->error ? 0 : nmemb;
}

static int fill_buffer(FILE *stream)
{
    ssize_t n = read(stream->fd, stream->buf, BUFSIZ);
    if (n <= 0) {
        if (n < 0) {
            stream->error = 1;
        } else {
            stream->flags |= _F_EOF;
        }
        stream->rpos = stream->rlen = 0;
        return -1;
    }
    stream->rpos = 0;
    stream->rlen = (size_t)n;
    return 0;
}

size_t fread(void *ptr, size_t size, size_t nmemb, FILE *stream)
{
    if (size == 0 || nmemb == 0) {
        return 0;
    }
    if (!(stream->flags & _F_READ)) {
        stream->error = 1;
        return 0;
    }
    unsigned char *out = ptr;
    size_t want = size * nmemb;
    size_t done = 0;
    while (done < want) {
        if (stream->rpos < stream->rlen) {
            size_t take = stream->rlen - stream->rpos;
            if (take > want - done) {
                take = want - done;
            }
            memcpy(out + done, stream->buf + stream->rpos, take);
            stream->rpos += take;
            done += take;
            continue;
        }
        if (stream->flags & _F_EOF) {
            break;
        }
        /* Large remaining reads bypass the buffer. */
        if (want - done >= BUFSIZ && (size == 1 || (want - done) % size == 0)) {
            ssize_t n = read(stream->fd, out + done, want - done);
            if (n <= 0) {
                if (n < 0) {
                    stream->error = 1;
                } else {
                    stream->flags |= _F_EOF;
                }
                break;
            }
            done += (size_t)n;
            continue;
        }
        if (fill_buffer(stream) != 0) {
            break;
        }
    }
    return done / size;
}

int fgetc(FILE *stream)
{
    unsigned char c;
    if (fread(&c, 1, 1, stream) != 1) {
        return EOF;
    }
    return c;
}

int getc(FILE *stream)
{
    return fgetc(stream);
}

int getchar(void)
{
    return fgetc(stdin);
}

int ungetc(int c, FILE *stream)
{
    if (c == EOF) {
        return EOF;
    }
    if (stream->rpos > 0) {
        stream->buf[--stream->rpos] = (unsigned char)c;
    } else if (stream->rlen < BUFSIZ) {
        memmove(stream->buf + 1, stream->buf, stream->rlen);
        stream->buf[0] = (unsigned char)c;
        stream->rlen++;
    } else {
        return EOF;
    }
    stream->flags &= ~_F_EOF;
    return c;
}

char *fgets(char *s, int size, FILE *stream)
{
    int i = 0;
    if (size <= 0) {
        return NULL;
    }
    while (i < size - 1) {
        int c = fgetc(stream);
        if (c == EOF) {
            break;
        }
        s[i++] = (char)c;
        if (c == '\n') {
            break;
        }
    }
    s[i] = 0;
    return i == 0 ? NULL : s;
}

static ssize_t getdelim_impl(char **lineptr, size_t *n, int delim, FILE *stream)
{
    size_t len = 0;
    if (*lineptr == NULL || *n == 0) {
        *n = 128;
        *lineptr = malloc(*n);
        if (!*lineptr) {
            return -1;
        }
    }
    for (;;) {
        int c = fgetc(stream);
        if (c == EOF) {
            if (len == 0) {
                return -1;
            }
            break;
        }
        if (len + 2 > *n) {
            size_t bigger = *n * 2;
            char *p = realloc(*lineptr, bigger);
            if (!p) {
                return -1;
            }
            *lineptr = p;
            *n = bigger;
        }
        (*lineptr)[len++] = (char)c;
        if (c == delim) {
            break;
        }
    }
    (*lineptr)[len] = 0;
    return (ssize_t)len;
}

ssize_t getdelim(char **lineptr, size_t *n, int delim, FILE *stream)
{
    return getdelim_impl(lineptr, n, delim, stream);
}

ssize_t getline(char **lineptr, size_t *n, FILE *stream)
{
    return getdelim_impl(lineptr, n, '\n', stream);
}
