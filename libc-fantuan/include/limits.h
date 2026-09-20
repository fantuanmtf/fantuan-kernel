/* libc-fantuan — limits.h (P1). */
#ifndef _LIMITS_H
#define _LIMITS_H

#define CHAR_BIT 8
#define SCHAR_MIN (-128)
#define SCHAR_MAX 127
#define UCHAR_MAX 255
#define CHAR_MIN SCHAR_MIN
#define CHAR_MAX SCHAR_MAX
#define SHRT_MIN (-32768)
#define SHRT_MAX 32767
#define USHRT_MAX 65535
#define INT_MIN (-2147483647 - 1)
#define INT_MAX 2147483647
#define UINT_MAX 4294967295U
#define LONG_MIN (-__LONG_MAX__ - 1L)
#define LONG_MAX __LONG_MAX__
#define ULONG_MAX (__LONG_MAX__ * 2UL + 1UL)
#define LLONG_MIN (-__LONG_LONG_MAX__ - 1LL)
#define LLONG_MAX __LONG_LONG_MAX__
#define ULLONG_MAX (__LONG_LONG_MAX__ * 2ULL + 1ULL)
#define SSIZE_MAX LONG_MAX
#define MB_LEN_MAX 1
/* Fantuan tmpfs limits (abi + kernel-core::vfs::tmpfs): paths are 255 bytes
 * plus NUL; names are 31; each file is capped at 8 KiB in P1. */
#define PATH_MAX 256
#define NAME_MAX 31
#define OPEN_MAX 16
#define PIPE_BUF 512
#define ARG_MAX 4096
#define NGROUPS_MAX 0

#endif /* _LIMITS_H */
