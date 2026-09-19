/* rump_shim_time.c - PIT tick, SPL masking and interrupt state (ours).
 * The PIT runs at HZ; the kernel calls rump_shim_tick() from IRQ0, which
 * updates the NetBSD clock globals and drives the callout wheel.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/kernel.h>
#include <sys/timevar.h>
#include <sys/callout.h>
#include <sys/cpu.h>
#include <machine/intr.h>
#include "rump_shim.h"

int hz = 100;
int tick = 1000000 / 100;
int stathz = 0;
int profhz = 0;

volatile time_t time_uptime;

static volatile int intr_depth;

int
getticks(void)
{

	return (int)(fantuan_rump_ticks() & 0x7fffffff);
}

void
rump_shim_tick(void)
{

	intr_depth++;
	time_uptime = (time_t)(fantuan_rump_ticks() / (uint64_t)hz);
	callout_hardclock();
	intr_depth--;
}

bool
cpu_intr_p(void)
{

	return intr_depth != 0;
}

bool
cpu_softintr_p(void)
{

	return false;
}

/*
 * Interrupt masking. The cookie's bit 8 records whether IF was set, so a
 * nested splx() only re-enables when the outermost mask has been dropped.
 */
int
splraise(int ipl)
{
	unsigned long flags;

	(void)ipl;
	__asm__ volatile("pushfq; popq %0; cli" : "=r"(flags) :: "memory");
	return (flags & 0x200ul) != 0 ? 0x100 : 0;
}

void
spllower(int s)
{

	if ((s & 0x100) != 0)
		__asm__ volatile("sti" ::: "memory");
}
