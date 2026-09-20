/* libc-fantuan — C89/C99 freestanding subset (P1). */
#ifndef _ERRNO_H
#define _ERRNO_H

#include <fantuan/abi.h>

extern int errno;

/* Fantuan-native errno values: a syscall returns -errno, and the wrappers
 * set errno from it. These are NOT Linux's numbers; they match
 * abi/src/lib.rs (SYS_ERR_*). */
#define EPERM 27
#define ENOENT 3
#define ESRCH 28
#define EINTR 25
#define EIO 5
#define ENXIO 6
#define E2BIG 29
#define ENOEXEC 30
#define EBADF 4
#define ECHILD 26
#define EAGAIN 17
#define ENOMEM 6
#define EACCES 7
#define EFAULT 16
#define EBUSY 31
#define EEXIST 8
#define ENODEV 20
#define ENOTDIR 9
#define EISDIR 10
#define EINVAL 2
#define ENFILE 32
#define EMFILE 15
#define ENOTTY 14
#define EFBIG 23
#define ENOSPC 24
#define ESPIPE 13
#define EROFS 19
#define EMLINK 33
#define EPIPE 18
#define EDOM 34
#define ERANGE 12
#define ENOSYS 1
#define ENOTEMPTY 11
#define ELOOP 22
#define ENAMETOOLONG 21
#define EOVERFLOW 35
#define EWOULDBLOCK EAGAIN
#define EDEADLK 36

#endif /* _ERRNO_H */
