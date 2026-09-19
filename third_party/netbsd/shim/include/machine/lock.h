/* fantuan adaptation shim: x86_64 simple locks (not upstream NetBSD).
 * The real x86 header uses inline asm; the shim uses compiler atomics.
 */
#ifndef FANTUAN_MACHINE_LOCK_H
#define FANTUAN_MACHINE_LOCK_H

#include <sys/param.h>

static __inline int
__SIMPLELOCK_LOCKED_P(const __cpu_simple_lock_t *p)
{
    return *p == __SIMPLELOCK_LOCKED;
}

static __inline int
__SIMPLELOCK_UNLOCKED_P(const __cpu_simple_lock_t *p)
{
    return *p == __SIMPLELOCK_UNLOCKED;
}

static __inline void
__cpu_simple_lock_set(__cpu_simple_lock_t *p)
{
    *p = __SIMPLELOCK_LOCKED;
}

static __inline void
__cpu_simple_lock_clear(__cpu_simple_lock_t *p)
{
    *p = __SIMPLELOCK_UNLOCKED;
}

static __inline void
__cpu_simple_lock_init(__cpu_simple_lock_t *p)
{
    *p = __SIMPLELOCK_UNLOCKED;
}

static __inline int
__cpu_simple_lock_try(__cpu_simple_lock_t *p)
{
    unsigned char expected = __SIMPLELOCK_UNLOCKED;
    return __atomic_compare_exchange_n(p, &expected, __SIMPLELOCK_LOCKED,
        0, __ATOMIC_ACQUIRE, __ATOMIC_RELAXED);
}

static __inline void
__cpu_simple_lock(__cpu_simple_lock_t *p)
{
    while (!__cpu_simple_lock_try(p))
        continue;
}

static __inline void
__cpu_simple_unlock(__cpu_simple_lock_t *p)
{
    __atomic_store_n(p, __SIMPLELOCK_UNLOCKED, __ATOMIC_RELEASE);
}

#endif
