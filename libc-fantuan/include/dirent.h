/* libc-fantuan — dirent.h (P1). One Fantuan-native 72-byte record per
 * getdents call; d_name is 56 bytes (matches abi/src/posix.rs). */
#ifndef _DIRENT_H
#define _DIRENT_H

#include <sys/types.h>

#define DT_UNKNOWN 0
#define DT_FIFO 1
#define DT_CHR 2
#define DT_DIR 4
#define DT_BLK 6
#define DT_REG 8
#define DT_LNK 10
#define DT_SOCK 12

struct dirent {
    ino_t d_ino;
    unsigned int d_type;
    unsigned int d_reclen;
    char d_name[56];
};

typedef struct _DIR DIR;

DIR *opendir(const char *name);
DIR *fdopendir(int fd);
struct dirent *readdir(DIR *dirp);
int closedir(DIR *dirp);
void rewinddir(DIR *dirp);
int dirfd(DIR *dirp);

#endif /* _DIRENT_H */
