/* rump_shim_init.c - ordered bring-up of the imported NetBSD slice (ours).
 * Order matters: kmem/vmem must exist before the pool subsystem, event
 * counters before the callout CPU (which attaches dynamic evcnts), and the
 * callout wheel before mbinit starts issuing mbuf pool allocations.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/kernel.h>
#include <sys/evcnt.h>
#include <sys/pool.h>
#include <sys/callout.h>
#include <sys/cpu.h>
#include <sys/mbuf.h>
#include "rump_shim.h"

void rump_shim_init_cpu(void);

extern psize_t physmem;
extern int nkmempages;
extern int cold;

void
rump_shim_init(void)
{

	rump_shim_init_cpu();
	physmem = (psize_t)fantuan_rump_physmem_pages();
	nkmempages = (int)(physmem / 4);

	pool_subsystem_init();
	evcnt_init();
	callout_startup();
	callout_init_cpu(&cpu_info_primary);
	mbinit();

	cold = 0;
}
