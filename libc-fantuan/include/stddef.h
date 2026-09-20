/* libc-fantuan — stddef.h (P1). Shadows the compiler header so the include
 * path is self-contained (the build also passes clang's resource include). */
#ifndef _STDDEF_H
#define _STDDEF_H

typedef __SIZE_TYPE__ size_t;
typedef __PTRDIFF_TYPE__ ptrdiff_t;
typedef __WCHAR_TYPE__ wchar_t;
typedef struct {
    long long __a;
    long double __b;
} max_align_t;

#define NULL ((void *)0)
#define offsetof(type, member) __builtin_offsetof(type, member)

#endif /* _STDDEF_H */
