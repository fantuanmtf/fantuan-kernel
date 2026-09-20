/* libc-fantuan — integer helpers, rand, qsort/bsearch (P1).
 *
 * Split from stdlib.c to keep files within the repo's 300-line convention. */
#include <errno.h>
#include <limits.h>
#include <stdlib.h>

int abs(int j) { return j < 0 ? -j : j; }
long labs(long j) { return j < 0 ? -j : j; }
long long llabs(long long j) { return j < 0 ? -j : j; }

static unsigned long rand_state = 1;
int rand(void)
{
    rand_state = rand_state * 1103515245UL + 12345UL;
    return (int)((rand_state >> 16) & RAND_MAX);
}

void srand(unsigned int seed) { rand_state = seed; }

static void swap_bytes(char *a, char *b, size_t size)
{
    while (size--) {
        char t = *a;
        *a++ = *b;
        *b++ = t;
    }
}

void qsort(void *base, size_t nmemb, size_t size, int (*compar)(const void *, const void *))
{
    char *p = base;
    for (size_t i = 1; i < nmemb; i++) {
        for (size_t j = i; j > 0 && compar(p + (j - 1) * size, p + j * size) > 0; j--) {
            swap_bytes(p + (j - 1) * size, p + j * size, size);
        }
    }
}

void *bsearch(const void *key, const void *base, size_t nmemb, size_t size,
              int (*compar)(const void *, const void *))
{
    const char *p = base;
    size_t lo = 0, hi = nmemb;
    while (lo < hi) {
        size_t mid = (lo + hi) / 2;
        int c = compar(key, p + mid * size);
        if (c == 0) {
            return (void *)(p + mid * size);
        }
        if (c < 0) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    return NULL;
}

int mkstemp(char *template)
{
    (void)template; /* P1: no O_EXCL randomness yet */
    errno = ENOSYS;
    return -1;
}

char *mktemp(char *template)
{
    (void)template;
    errno = ENOSYS;
    return NULL;
}

char *realpath(const char *path, char *resolved_path)
{
    (void)path;
    (void)resolved_path;
    errno = ENOSYS;
    return NULL;
}

int system(const char *command)
{
    if (command == NULL) {
        return 1; /* a shell is not running */
    }
    errno = ENOSYS;
    return -1;
}
