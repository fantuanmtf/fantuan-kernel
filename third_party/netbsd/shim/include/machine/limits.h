/* fantuan adaptation shim: x86_64 limits.h (not upstream NetBSD). */
#ifndef FANTUAN_MACHINE_LIMITS_H
#define FANTUAN_MACHINE_LIMITS_H

#define CHAR_BIT 8
#define UCHAR_MAX 0xff
#define SCHAR_MAX 0x7f
#define SCHAR_MIN (-0x7f - 1)
#define USHRT_MAX 0xffff
#define SHRT_MAX 0x7fff
#define SHRT_MIN (-0x7fff - 1)
#define UINT_MAX 0xffffffffU
#define INT_MAX 0x7fffffff
#define INT_MIN (-0x7fffffff - 1)
#define ULONG_MAX 0xffffffffffffffffUL
#define LONG_MAX 0x7fffffffffffffffL
#define LONG_MIN (-0x7fffffffffffffffL - 1)
#define ULLONG_MAX 0xffffffffffffffffULL
#define LLONG_MAX 0x7fffffffffffffffLL
#define LLONG_MIN (-0x7fffffffffffffffLL - 1)
#define SSIZE_MAX LONG_MAX
#define SSIZE_MIN LONG_MIN
#define SIZE_T_MAX ULONG_MAX
#define LONG_BIT 64
#define WORD_BIT 32
#define DBL_DIG __DBL_DIG__
#define DBL_MAX __DBL_MAX__
#define DBL_MIN __DBL_MIN__
#define FLT_DIG __FLT_DIG__
#define FLT_MAX __FLT_MAX__
#define FLT_MIN __FLT_MIN__
#define MB_LEN_MAX 32
#include <machine/wchar_limits.h>
#endif
