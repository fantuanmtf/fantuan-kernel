/* fantuan adaptation shim: x86_64 cycle counter (not upstream NetBSD). */
#ifndef FANTUAN_MACHINE_CPU_COUNTER_H
#define FANTUAN_MACHINE_CPU_COUNTER_H

#include <sys/types.h>

struct cpu_info;

extern uint64_t cpu_frequency(struct cpu_info *);
extern int cpu_hascounter(void);
extern uint64_t (*cpu_counter)(void);
extern uint32_t (*cpu_counter32)(void);

#endif
