/* libc-fantuan — stdlib.h (P1). */
#ifndef _STDLIB_H
#define _STDLIB_H

#include <stddef.h>

#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1
#define RAND_MAX 32767
#define MB_CUR_MAX 1

void *malloc(size_t size);
void *calloc(size_t nmemb, size_t size);
void *realloc(void *ptr, size_t size);
void free(void *ptr);

void exit(int status) __attribute__((noreturn));
void abort(void) __attribute__((noreturn));
void _Exit(int status) __attribute__((noreturn));
int atexit(void (*func)(void));

int atoi(const char *nptr);
long atol(const char *nptr);
long long atoll(const char *nptr);
double atof(const char *nptr);
long strtol(const char *nptr, char **endptr, int base);
unsigned long strtoul(const char *nptr, char **endptr, int base);
long long strtoll(const char *nptr, char **endptr, int base);
unsigned long long strtoull(const char *nptr, char **endptr, int base);
double strtod(const char *nptr, char **endptr);

char *getenv(const char *name);
int setenv(const char *name, const char *value, int overwrite);
int unsetenv(const char *name);
int putenv(char *string);

void qsort(void *base, size_t nmemb, size_t size, int (*compar)(const void *, const void *));
void *bsearch(const void *key, const void *base, size_t nmemb, size_t size,
              int (*compar)(const void *, const void *));

int abs(int j);
long labs(long j);
long long llabs(long long j);
int rand(void);
void srand(unsigned int seed);

int mkstemp(char *template);
char *mktemp(char *template);
char *realpath(const char *path, char *resolved_path);
int system(const char *command);
unsigned long strtoul(const char *nptr, char **endptr, int base);

/* job control / process (P2; ENOSYS stubs in P1) */
int posix_memalign(void **memptr, size_t alignment, size_t size);

#endif /* _STDLIB_H */
