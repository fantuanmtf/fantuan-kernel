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

/* P3 additions (beyond the kernel's SYS_ERR_* set). */
#define EILSEQ 37
#define ENOTSOCK 38
#define EPROTONOSUPPORT 39
#define EAFNOSUPPORT 40
#define EOPNOTSUPP 41
#define ENOTSUP EOPNOTSUPP
#define ENOBUFS 42
#define ETIMEDOUT 43
#define ECONNREFUSED 44
#define ECONNRESET 47
#define EHOSTUNREACH 45
#define ENETUNREACH 46
#define EISCONN 48
#define ENOTCONN 49
#define EINPROGRESS 50

#endif /* _ERRNO_H */
