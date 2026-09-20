/* rump_shim_sleepq.c - cooperative sleep-queue and turnstile stubs (ours).
 * The MI lock/condvar code only calls these when a lock is actually
 * contended; on the pre-SMP cooperative model that must assert rather than
 * spin or block. sleepq_locks is real storage so the hashlock helpers can
 * still take their (uncontended) spin mutex.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/lwp.h>
#include <sys/sched.h>
#include <sys/sleepq.h>
#include <sys/syncobj.h>
#include "rump_shim.h"

void
sleepq_init(sleepq_t *sq)
{

	memset(sq, 0, sizeof(*sq));
}

void
sleepq_remove(sleepq_t *sq, lwp_t *l)
{

	(void)sq;
	(void)l;
	panic("sleepq_remove: blocking unsupported (R2 cooperative locks)");
}

void
sleepq_enqueue(sleepq_t *sq, wchan_t chan, const char *wmesg,
    struct syncobj *sobj, bool catch_p)
{

	(void)sq;
	(void)chan;
	(void)sobj;
	(void)catch_p;
	panic("sleepq_enqueue(%s): blocking unsupported (R2 cooperative locks)",
	    wmesg != NULL ? wmesg : "?");
}

void
sleepq_unsleep(lwp_t *l, bool cleanup)
{

	(void)l;
	(void)cleanup;
	panic("sleepq_unsleep: blocking unsupported (R2 cooperative locks)");
}

int
sleepq_block(int timo, bool catch_p, struct syncobj *sobj)
{

	(void)timo;
	(void)catch_p;
	(void)sobj;
	panic("sleepq_block: blocking unsupported (R2 cooperative locks)");
}

void
sleepq_wake(sleepq_t *sq, wchan_t chan, u_int n, kmutex_t *mp)
{

	(void)sq;
	(void)chan;
	(void)n;
	(void)mp;
	panic("sleepq_wake: blocking unsupported (R2 cooperative locks)");
}

void
sleepq_changepri(lwp_t *l, pri_t pri)
{

	(void)l;
	(void)pri;
}

void
sleepq_lendpri(lwp_t *l, pri_t pri)
{

	(void)l;
	(void)pri;
}

turnstile_t *
turnstile_lookup(wchan_t chan)
{

	(void)chan;
	return NULL;
}

void
turnstile_exit(wchan_t chan)
{

	(void)chan;
}

void
turnstile_block(turnstile_t *ts, int q, wchan_t chan, syncobj_t *sobj)
{
	kmutex_t *mtx = (kmutex_t *)(uintptr_t)chan;
	int i;

	(void)ts;
	(void)q;
	(void)sobj;
	/*
	 * Cooperative single-lwp model: the "contended" lock is owned by
	 * another kernel task (which the scheduler may have switched out).
	 * Yield until it is released; the mutex_enter loop re-checks the
	 * owner after every return.  A genuinely recursive acquisition
	 * never releases, so the bound turns that deadlock into a panic.
	 */
	for (i = 0; i < 1000; i++) {
		if (mtx == NULL || mtx->u.mtxa_owner == 0)
			return;
		fantuan_rump_yield();
	}
	panic("turnstile_block: lock %p still held after 1000 yields", chan);
}

void
turnstile_wakeup(turnstile_t *ts, int q, int n, lwp_t *l)
{

	(void)ts;
	(void)q;
	(void)n;
	(void)l;
}

void
turnstile_unsleep(lwp_t *l, bool cleanup)
{

	(void)l;
	(void)cleanup;
}

void
turnstile_changepri(lwp_t *l, pri_t pri)
{

	(void)l;
	(void)pri;
}

struct lwp *
syncobj_noowner(wchan_t chan)
{

	(void)chan;
	return NULL;
}

syncobj_t sleep_syncobj = {
	.sobj_flag = SOBJ_SLEEPQ_SORTED,
	.sobj_unsleep = sleepq_unsleep,
	.sobj_changepri = sleepq_changepri,
	.sobj_lendpri = sleepq_lendpri,
	.sobj_owner = syncobj_noowner,
};
