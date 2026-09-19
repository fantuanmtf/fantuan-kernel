/* rump_shim_lock.c - single-CPU lock/sleep/task services (ours).
 * The MI lock code in kern_mutex.c/kern_rwlock.c runs its uncontended fast
 * paths unchanged; this file supplies the LWP/cpu sentinel it needs, the
 * xcall/percpu stubs, and asserts out of every state where a real block
 * would be required (pre-SMP cooperative model).
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/kmem.h>
#include <sys/mutex.h>
#include <sys/lwp.h>
#include <sys/proc.h>
#include <sys/sched.h>
#include <sys/sleepq.h>
#include <sys/syncobj.h>
#include <sys/pserialize.h>
#include <sys/lockdebug.h>
#include <sys/systm.h>
#include <sys/kernel.h>
#include <sys/xcall.h>
#include <sys/percpu.h>
#include <machine/cpu.h>
#include "rump_shim.h"

struct cpu_info cpu_info_primary;
struct lwp lwp0;
struct lwp *curlwp;

int ncpu = 1;
bool mp_online = false;
int cold = 1;

sleepqlock_t sleepq_locks[SLEEPTAB_HASH_SIZE];

bool
kpreempt(uintptr_t where)
{

	(void)where;
	return false;
}

uint64_t
lwp_pctr(void)
{

	return fantuan_rump_switch_count();
}

void
lwp_unlock_to(lwp_t *l, kmutex_t *mp)
{

	(void)l;
	mutex_enter(mp);
}

bool
pserialize_not_in_read_section(void)
{

	return true;
}

void
lockdebug_abort(const char *func, size_t line, const volatile void *lock,
    lockops_t *ops, const char *msg)
{

	panic("%s:%zu %s lock %p: %s", func, line,
	    ops != NULL ? ops->lo_name : "?", (const void *)lock, msg);
}

kmutex_t *
mutex_obj_alloc(kmutex_type_t type, int ipl)
{
	kmutex_t *m = kmem_alloc(sizeof(*m), KM_SLEEP);

	if (m == NULL)
		panic("mutex_obj_alloc: out of memory");
	mutex_init(m, type, ipl);
	return m;
}


uint64_t
xc_unicast(u_int flags, xcfunc_t fn, void *arg1, void *arg2,
    struct cpu_info *ci)
{

	(void)flags;
	(void)ci;
	if (fn != NULL)
		fn(arg1, arg2);
	return 0;
}

void
xc_wait(uint64_t where)
{

	(void)where;
}

void
xc_barrier(u_int flags)
{

	(void)flags;
}

unsigned int
xc_encode_ipl(int ipl)
{

	return (unsigned int)ipl;
}

uint64_t
xc_broadcast(u_int flags, xcfunc_t fn, void *arg1, void *arg2)
{

	(void)flags;
	if (fn != NULL)
		fn(arg1, arg2);
	return 0;
}

struct percpu {
	size_t p_size;
	void *p_data;
};

percpu_t *
percpu_alloc(size_t size)
{
	percpu_t *p = kmem_alloc(sizeof(*p), KM_SLEEP);

	if (p == NULL)
		return NULL;
	p->p_size = size;
	p->p_data = kmem_zalloc(size, KM_SLEEP);
	if (p->p_data == NULL)
		return NULL;
	return p;
}

void
percpu_free(percpu_t *p, size_t size)
{

	(void)p;
	(void)size;
}

void *
percpu_getref(percpu_t *p)
{

	return p->p_data;
}

void
percpu_putref(percpu_t *p)
{

	(void)p;
}

void
percpu_foreach(percpu_t *p, percpu_callback_t fn, void *arg)
{

	fn(p->p_data, arg, &cpu_info_primary);
}

void
rump_shim_init_cpu(void)
{

	cpu_info_primary.ci_self = &cpu_info_primary;
	cpu_info_primary.ci_name = "cpu0";
	cpu_info_primary.ci_curlwp = &lwp0;
	cpu_info_primary.ci_onproc = &lwp0;
	cpu_info_primary.ci_mtx_count = 0;
	lwp0.l_cpu = &cpu_info_primary;
	lwp0.l_stat = LSONPROC;
	lwp0.l_mutex = NULL;
	lwp0.l_proc = NULL;
	curlwp = &lwp0;
}
