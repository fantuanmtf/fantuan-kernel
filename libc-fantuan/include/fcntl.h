/* libc-fantuan — fcntl.h (P1). */
#ifndef _FCNTL_H
#define _FCNTL_H

#include <sys/types.h>

/* Values match abi/src/lib.rs. */
#define O_RDONLY 0
#define O_WRONLY 1
#define O_RDWR 2
#define O_CREAT 00000100
#define O_EXCL 00000200
#define O_TRUNC 00001000
#define O_APPEND 00002000
#define O_NONBLOCK 00004000
#define O_NOCTTY 00000400
#define O_CLOEXEC 02000000
#define O_BINARY 0

#define F_DUPFD 0
#define F_GETFD 1
#define F_SETFD 2
#define F_GETFL 3
#define F_SETFL 4
#define FD_CLOEXEC 1

int open(const char *path, int flags, ...);
int creat(const char *path, mode_t mode);
int fcntl(int fd, int cmd, ...);

#endif /* _FCNTL_H */
