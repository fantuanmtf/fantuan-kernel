/* libc-fantuan — mmap/munmap/mprotect (P2): anonymous private mappings over
 * SYS_MMAP2 (offset 0; file-backed and shared mappings are not supported). */
#include <errno.h>
#include <stddef.h>
#include <sys/mman.h>
#include <fantuan/abi.h>

long __fantuan_raw(long n, long a1, long a2, long a3, long a4, long a5);

static void *rc(long r)
{
    if (r < 0 && r > -4096) {
        errno = (int)-r;
        return MAP_FAILED;
    }
    return (void *)r;
}

void *mmap(void *addr, size_t length, int prot, int flags, int fd, off_t offset)
{
    if (offset != 0) {
        errno = ENOSYS; /* no file-backed mappings yet */
        return MAP_FAILED;
    }
    return rc(__fantuan_raw(FANTUAN_SYS_MMAP2, (long)addr, (long)length, prot,
                            flags, fd));
}

int munmap(void *addr, size_t length)
{
    long r = __fantuan_raw(FANTUAN_SYS_MUNMAP, (long)addr, (long)length, 0, 0, 0);
    if (r < 0 && r > -4096) {
        errno = (int)-r;
        return -1;
    }
    return 0;
}

int mprotect(void *addr, size_t len, int prot)
{
    long r = __fantuan_raw(FANTUAN_SYS_MPROTECT, (long)addr, (long)len, prot, 0, 0);
    if (r < 0 && r > -4096) {
        errno = (int)-r;
        return -1;
    }
    return 0;
}
