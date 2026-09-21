/* libc-fantuan — alloca.h (P2): compiler-provided stack allocation. */
#ifndef _ALLOCA_H
#define _ALLOCA_H

#include <stddef.h>

#define alloca(size) __builtin_alloca(size)

#endif /* _ALLOCA_H */
