/* fantuan adaptation shim: x86_64 machine types (not upstream NetBSD).
 * The real amd64 header delegates to sys/common_int_*.h and defines the
 * LP64 kernel address types; the feature macros below are the subset the
 * imported MI headers actually consult.
 */
#ifndef FANTUAN_MACHINE_TYPES_H
#define FANTUAN_MACHINE_TYPES_H

#include <sys/cdefs.h>
#include <sys/featuretest.h>
#include <machine/int_types.h>

#if defined(_KERNEL)
typedef struct label_t {
    long val[8];
} label_t;
#endif

typedef unsigned long paddr_t;
typedef unsigned long psize_t;
typedef unsigned long vaddr_t;
typedef unsigned long vsize_t;
typedef long int register_t;
typedef long int __register_t;
typedef unsigned char __cpu_simple_lock_nv_t;

#define __SIMPLELOCK_LOCKED 1
#define __SIMPLELOCK_UNLOCKED 0
#define __NO_STRICT_ALIGNMENT
#define __HAVE_ATOMIC64_OPS
#define __HAVE_ATOMIC_AS_MEMBAR
#define __HAVE_CPU_COUNTER
#define __HAVE_INTR_CONTROL
#define __HAVE_CPU_RNG
#define __HAVE_NEW_STYLE_BUS_H

#endif
