/* fantuan adaptation shim: x86_64 interrupt priority levels (not upstream). */
#ifndef FANTUAN_MACHINE_INTRDEFS_H
#define FANTUAN_MACHINE_INTRDEFS_H

#define IPL_NONE 0x0
#define IPL_PREEMPT 0x1
#define IPL_SOFTCLOCK 0x2
#define IPL_SOFTBIO 0x3
#define IPL_SOFTNET 0x4
#define IPL_SOFTSERIAL 0x5
#define IPL_VM 0x6
#define IPL_SCHED 0x7
#define IPL_HIGH 0x8
#define NIPL 9

#endif
