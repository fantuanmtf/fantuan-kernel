/* libc-fantuan — dirent (P1): one 72-byte Fantuan record per getdents call. */
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdlib.h>
#include <unistd.h>
#include <fantuan/abi.h>

struct _DIR {
    int fd;
    struct dirent de;
};

DIR *fdopendir(int fd)
{
    DIR *d = malloc(sizeof(DIR));
    if (!d) {
        close(fd);
        return NULL;
    }
    d->fd = fd;
    return d;
}

DIR *opendir(const char *name)
{
    int fd = open(name, O_RDONLY);
    if (fd < 0) {
        return NULL;
    }
    return fdopendir(fd);
}

struct dirent *readdir(DIR *dirp)
{
    long r = __fantuan_syscall6(FANTUAN_SYS_GETDENTS, dirp->fd,
                               (long)&dirp->de, sizeof(dirp->de), 0, 0);
    if (r < 0) {
        errno = (int)-r;
        return NULL;
    }
    if (r == 0) {
        return NULL; /* end of directory */
    }
    dirp->de.d_name[sizeof(dirp->de.d_name) - 1] = 0;
    return &dirp->de;
}

int closedir(DIR *dirp)
{
    int fd = dirp->fd;
    free(dirp);
    return close(fd);
}

void rewinddir(DIR *dirp)
{
    (void)lseek(dirp->fd, 0, SEEK_SET);
}

int dirfd(DIR *dirp)
{
    return dirp->fd;
}
