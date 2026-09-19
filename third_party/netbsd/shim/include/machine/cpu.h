/* fantuan adaptation shim: x86_64 CPU info (not upstream NetBSD).
 * The real x86 header describes the full AP/IDT/APIC state; the spike only
 * needs the MI-visible fields (curcpu()->ci_index, ci_curlwp, ...).
 */
#ifndef FANTUAN_MACHINE_CPU_H
#define FANTUAN_MACHINE_CPU_H

#include <sys/types.h>
#include <sys/cpu_data.h>

struct lwp;

struct cpu_info {
    struct cpu_data ci_data;
    struct cpu_info *ci_self;
    struct lwp *ci_curlwp;
    struct lwp *ci_onproc;
    const char *ci_name;
    int ci_ilevel;
    int ci_mtx_count;
    int ci_mtx_oldspl;
    int ci_nintrhand;
    int ci_loclen;
    const void *ci_locdesc;
};

extern struct cpu_info cpu_info_primary;

#define curcpu() (&cpu_info_primary)
#define cpu_number() (curcpu()->ci_index)
#define CPU_IS_PRIMARY(ci) ((ci) == &cpu_info_primary)

#endif
