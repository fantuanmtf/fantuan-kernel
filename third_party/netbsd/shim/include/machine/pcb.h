/* fantuan adaptation shim: x86_64 pcb (not upstream NetBSD).
 * sizeof(struct pcb) only participates in KSTACK_SIZE in the imported MI
 * slice; the real context-switch frame is a port-time task.
 */
#ifndef FANTUAN_MACHINE_PCB_H
#define FANTUAN_MACHINE_PCB_H

#include <sys/types.h>

struct pcb {
    register_t pcb_rsp;
    register_t pcb_rbp;
    register_t pcb_rbx;
    register_t pcb_r12;
    register_t pcb_r13;
    register_t pcb_r14;
    register_t pcb_r15;
    register_t pcb_cr3;
};

#endif
