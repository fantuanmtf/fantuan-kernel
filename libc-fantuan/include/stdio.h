/* libc-fantuan — stdio.h (P1): buffered FILE over the fd syscalls. */
#ifndef _STDIO_H
#define _STDIO_H

#include <stddef.h>
#include <stdarg.h>
#include <sys/types.h>

#define EOF (-1)
#define BUFSIZ 256
#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2
#define FILENAME_MAX 256
#define FOPEN_MAX 16
#define TMP_MAX 0

typedef struct _FILE FILE;

struct _FILE {
    int fd;
    int flags; /* 1 = readable, 2 = writable, 4 = line buffered, 8 = eof */
    unsigned char buf[BUFSIZ];
    size_t rpos, rlen;
    size_t wlen;
    int error;
};

extern FILE *stdin;
extern FILE *stdout;
extern FILE *stderr;

FILE *fopen(const char *path, const char *mode);
FILE *fdopen(int fd, const char *mode);
FILE *freopen(const char *path, const char *mode, FILE *stream);
int fclose(FILE *stream);
int fflush(FILE *stream);
size_t fread(void *ptr, size_t size, size_t nmemb, FILE *stream);
size_t fwrite(const void *ptr, size_t size, size_t nmemb, FILE *stream);
int fgetc(FILE *stream);
int getc(FILE *stream);
int getchar(void);
int ungetc(int c, FILE *stream);
char *fgets(char *s, int size, FILE *stream);
ssize_t getline(char **lineptr, size_t *n, FILE *stream);
ssize_t getdelim(char **lineptr, size_t *n, int delim, FILE *stream);
int fputc(int c, FILE *stream);
int putc(int c, FILE *stream);
int putchar(int c);
int fputs(const char *s, FILE *stream);
int puts(const char *s);
int feof(FILE *stream);
int ferror(FILE *stream);
void clearerr(FILE *stream);
int fileno(FILE *stream);
void perror(const char *s);
int remove(const char *path);
int rename(const char *oldpath, const char *newpath);
FILE *tmpfile(void);
int setvbuf(FILE *stream, char *buf, int mode, size_t size);
void setbuf(FILE *stream, char *buf);

int printf(const char *fmt, ...);
int fprintf(FILE *stream, const char *fmt, ...);
int sprintf(char *buf, const char *fmt, ...);
int snprintf(char *buf, size_t size, const char *fmt, ...);
int vprintf(const char *fmt, va_list ap);
int vfprintf(FILE *stream, const char *fmt, va_list ap);
int vsprintf(char *buf, const char *fmt, va_list ap);
int vsnprintf(char *buf, size_t size, const char *fmt, va_list ap);
int scanf(const char *fmt, ...);
int sscanf(const char *s, const char *fmt, ...);

#define _IONBF 0
#define _IOLBF 1
#define _IOFBF 2

/* Internal FILE flags shared by stdio.c / stdio_file.c. */
#define _F_READ 1
#define _F_WRITE 2
#define _F_LINE 4
#define _F_EOF 8

#endif /* _STDIO_H */
