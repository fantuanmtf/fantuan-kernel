/* fantuan adaptation shim: x86_64 kmutex layout (not upstream NetBSD).
 * Layout-compatible with the real x86 header; the spike replaces the
 * lock primitives with compiler atomics (see ADAPTATION.md).
 */
#ifndef FANTUAN_MACHINE_MUTEX_H
#define FANTUAN_MACHINE_MUTEX_H

#include <sys/types.h>
#ifdef _KERNEL
#include <machine/intr.h>
#endif

struct kmutex {
    union {
        volatile uintptr_t mtxa_owner;
#ifdef _KERNEL
        struct {
            volatile uint8_t mtxs_dummy;
            ipl_cookie_t mtxs_ipl;
            __cpu_simple_lock_t mtxs_lock;
            volatile uint8_t mtxs_unused;
        } s;
#endif
    } u;
};

#ifdef __MUTEX_PRIVATE
#define mtx_owner u.mtxa_owner
#define mtx_ipl u.s.mtxs_ipl
#define mtx_lock u.s.mtxs_lock
#define __HAVE_MUTEX_STUBS 1
#define __HAVE_SPIN_MUTEX_STUBS 1
#define __HAVE_SIMPLE_MUTEXES 1
#define MUTEX_CAS(p, o, n) \
    (_atomic_cas_ulong((volatile unsigned long *)(p), (o), (n)) == (o))
unsigned long _atomic_cas_ulong(volatile unsigned long *, unsigned long,
    unsigned long);
#endif

#endif
