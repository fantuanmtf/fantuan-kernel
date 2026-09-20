/* libc-fantuan — malloc family (P1): a simple free-list over brk.
 *
 * Blocks carry a 16-byte header (size + free-list link). The arena comes
 * from SYS_BRK in 64 KiB chunks; free() inserts address-sorted and merges
 * adjacent free blocks. No threads, no mmap path (bash is built
 * --without-bash-malloc, so it uses this allocator). */
#include <errno.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fantuan/abi.h>

typedef struct block {
    size_t size; /* payload bytes */
    struct block *next;
} block_t;

#define HDR ((sizeof(block_t) + 15) & ~(size_t)15)
#define ALIGN16(n) (((n) + 15) & ~(size_t)15)
#define ARENA_CHUNK (64 * 1024)

static block_t *free_list;
static char *heap_ptr;   /* last break the kernel acknowledged */
static char *arena_ptr;  /* next free byte in the current chunk */
static size_t arena_left;

long __fantuan_raw(long n, long a1, long a2, long a3, long a4, long a5);

void *__fantuan_sbrk(long increment)
{
    if (heap_ptr == NULL) {
        long base = __fantuan_raw(FANTUAN_SYS_BRK, 0, 0, 0, 0, 0);
        if (base <= 0) {
            errno = ENOMEM;
            return NULL;
        }
        heap_ptr = (char *)base;
    }
    char *want = heap_ptr + increment;
    long got = __fantuan_raw(FANTUAN_SYS_BRK, (long)want, 0, 0, 0, 0);
    if (got < 0 || (char *)got != want) {
        errno = ENOMEM;
        return NULL;
    }
    char *old = heap_ptr;
    heap_ptr = want;
    return old;
}

static void *bump(size_t need)
{
    if (arena_left < need + HDR) {
        size_t chunk = need + HDR + ARENA_CHUNK;
        chunk = (chunk + 4095) & ~(size_t)4095;
        char *p = __fantuan_sbrk((long)chunk);
        if (p == NULL) {
            return NULL;
        }
        arena_ptr = p;
        arena_left = chunk;
    }
    block_t *b = (block_t *)arena_ptr;
    b->size = need;
    b->next = NULL;
    arena_ptr += need + HDR;
    arena_left -= need + HDR;
    return (char *)b + HDR;
}

void *malloc(size_t size)
{
    if (size == 0) {
        size = 1;
    }
    size = ALIGN16(size);
    block_t **pp = &free_list;
    block_t *prev = NULL;
    while (*pp) {
        block_t *b = *pp;
        if (b->size >= size) {
            *pp = b->next;
            b->next = NULL;
            (void)prev;
            return (char *)b + HDR;
        }
        prev = b;
        pp = &b->next;
    }
    return bump(size);
}

void free(void *ptr)
{
    if (ptr == NULL) {
        return;
    }
    block_t *b = (block_t *)((char *)ptr - HDR);
    block_t **pp = &free_list;
    block_t *prev = NULL;
    while (*pp && (char *)*pp < (char *)b) {
        prev = *pp;
        pp = &(*pp)->next;
    }
    b->next = *pp;
    *pp = b;
    /* Merge with the following block when physically adjacent. */
    if (b->next && (char *)b + HDR + b->size == (char *)b->next) {
        b->size += HDR + b->next->size;
        b->next = b->next->next;
    }
    /* Merge with the preceding block. */
    if (prev && (char *)prev + HDR + prev->size == (char *)b) {
        prev->size += HDR + b->size;
        prev->next = b->next;
    }
}

void *calloc(size_t nmemb, size_t size)
{
    if (nmemb != 0 && size > (size_t)-1 / nmemb) {
        errno = ENOMEM;
        return NULL;
    }
    size_t total = nmemb * size;
    void *p = malloc(total);
    if (p) {
        memset(p, 0, total);
    }
    return p;
}

void *realloc(void *ptr, size_t size)
{
    if (ptr == NULL) {
        return malloc(size);
    }
    if (size == 0) {
        free(ptr);
        return NULL;
    }
    block_t *b = (block_t *)((char *)ptr - HDR);
    if (b->size >= size) {
        return ptr;
    }
    void *p = malloc(size);
    if (p == NULL) {
        return NULL;
    }
    memcpy(p, ptr, b->size);
    free(ptr);
    return p;
}

int posix_memalign(void **memptr, size_t alignment, size_t size)
{
    (void)alignment; /* 16-byte blocks satisfy the P1 callers */
    void *p = malloc(size);
    if (p == NULL) {
        return ENOMEM;
    }
    *memptr = p;
    return 0;
}
