/* fantuan adaptation shim: x86_64 proc/lwp substructs (not upstream NetBSD). */
#ifndef FANTUAN_MACHINE_PROC_H
#define FANTUAN_MACHINE_PROC_H

#include <sys/types.h>
#include <sys/time.h>

struct trapframe;

struct mdlwp {
    volatile uint64_t md_tsc;
    struct trapframe *md_regs;
    int md_flags;
    volatile int md_astpending;
};

struct mdproc {
    int md_flags;
    void (*md_syscall)(struct trapframe *);
};

#endif
