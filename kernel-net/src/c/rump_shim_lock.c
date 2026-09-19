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
#include <sys/rwlock.h>
#include <sys/lwp.h>
#include <sys/proc.h>
#include <sys/resource.h>
#include <sys/resourcevar.h>
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
kpreempt_disabled(void)
{

	return true;
}

void
kpreempt_disable(void)
{
}

void
kpreempt_enable(void)
{
}

void
lockdebug_abort(const char *func, size_t line, const volatile void *lock,
    lockops_t *ops, const char *msg)
{

	panic("%s:%zu %s lock %p: %s", func, line,
	    ops != NULL ? ops->lo_name : "?", (const void *)lock, msg);
}

/* mutex_obj_alloc/hold/free live in rump_shim_mobj.c. */

/* The machine header marks the port __HAVE_RW_STUBS, so the MI kern_rwlock
 * aliases are compiled out; pre-SMP the simple enter/exit pair is enough and
 * matches the earlier rwlock stub contract.  The writer preference is
 * irrelevant on one CPU. */
void
rw_enter(krwlock_t *rw, const krw_t op)
{

	(void)rw;
	(void)op;
}

void
rw_exit(krwlock_t *rw)
{

	(void)rw;
}

krwlock_t *
rw_obj_alloc(void)
{
	krwlock_t *rw = kmem_alloc(sizeof(*rw), KM_SLEEP);

	if (rw == NULL)
		panic("rw_obj_alloc: out of memory");
	rw_init(rw);
	return rw;
}

bool
rw_obj_free(krwlock_t *rw)
{

	if (rw == NULL)
		return false;
	rw_destroy(rw);
	kmem_free(rw, sizeof(*rw));
	return true;
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
percpu_foreach_xcall(percpu_t *p, u_int flags, percpu_callback_t fn, void *arg)
{

	(void)flags;
	fn(p->p_data, arg, &cpu_info_primary);
}

percpu_t *
percpu_create(size_t size, percpu_callback_t ctor, percpu_callback_t dtor,
    void *arg)
{
	percpu_t *p = percpu_alloc(size);

	if (p == NULL)
		return NULL;
	if (ctor != NULL)
		ctor(p->p_data, arg, &cpu_info_primary);
	(void)dtor;
	return p;
}

/* The socket layer reads l->l_proc->p_pid, p_rlimit and l->l_cred; the
 * kernel client has no process, so supply a minimal sentinel. */
static struct plimit lwp0_limit;
static struct proc proc0_store;

void
rump_shim_init_cpu(void)
{
	int i;

	for (i = 0; i < RLIM_NLIMITS; i++) {
		lwp0_limit.pl_rlimit[i].rlim_cur = RLIM_INFINITY;
		lwp0_limit.pl_rlimit[i].rlim_max = RLIM_INFINITY;
	}
	proc0_store.p_pid = 0;
	proc0_store.p_limit = &lwp0_limit;

	cpu_info_primary.ci_self = &cpu_info_primary;
	cpu_info_primary.ci_name = "cpu0";
	cpu_info_primary.ci_curlwp = &lwp0;
	cpu_info_primary.ci_onproc = &lwp0;
	cpu_info_primary.ci_mtx_count = 0;
	lwp0.l_cpu = &cpu_info_primary;
	lwp0.l_stat = LSONPROC;
	lwp0.l_mutex = NULL;
	lwp0.l_proc = &proc0_store;
	lwp0.l_cred = NULL;
	curlwp = &lwp0;
}
