/* rump_shim_if.c - ifnet-core services for the R3 loopback path (ours).
 * The MI ifnet core and the loopback driver are the real NetBSD files; this
 * file supplies the thread-context workqueue the route/if timers use and the
 * packet-filter hooks.  All deferred work runs synchronously in the caller's
 * task context: the cooperative single-CPU model has no worker threads yet.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include <sys/kmem.h>
#include <sys/socket.h>
#include <sys/kernel.h>
#include <sys/hook.h>
#include <sys/workqueue.h>
#include <sys/percpu.h>
#include <net/if.h>
#include <net/pfil.h>
#include "rump_shim.h"

struct workqueue {
	void (*wq_func)(struct work *, void *);
	void *wq_arg;
};

int
workqueue_create(struct workqueue **wqp, const char *name,
    void (*func)(struct work *, void *), void *arg, pri_t pri, int ipl,
    int flags)
{
	struct workqueue *wq;

	(void)name;
	(void)pri;
	(void)ipl;
	(void)flags;
	wq = kmem_alloc(sizeof(*wq), KM_SLEEP);
	if (wq == NULL)
		return ENOMEM;
	wq->wq_func = func;
	wq->wq_arg = arg;
	*wqp = wq;
	return 0;
}

void
workqueue_destroy(struct workqueue *wq)
{

	kmem_free(wq, sizeof(*wq));
}

void
workqueue_enqueue(struct workqueue *wq, struct work *wk, struct cpu_info *ci)
{

	(void)ci;
	if (wq != NULL && wq->wq_func != NULL)
		wq->wq_func(wk, wq->wq_arg);
}

void
workqueue_wait(struct workqueue *wq, struct work *wk)
{

	(void)wq;
	(void)wk;
}

struct pfil_head {
	int ph_dummy;
};

static struct pfil_head if_pfil_store;

pfil_head_t *
pfil_head_create(int type, void *key)
{

	(void)type;
	(void)key;
	return &if_pfil_store;
}

void
pfil_head_destroy(pfil_head_t *ph)
{

	(void)ph;
}

void
pfil_run_ifhooks(pfil_head_t *ph, unsigned long cmd, struct ifnet *ifp)
{

	(void)ph;
	(void)cmd;
	(void)ifp;
}

struct khook_list {
	int khl_dummy;
};

#define KHOOK_LISTS 8
static struct khook_list khook_lists[KHOOK_LISTS];
static int khook_next;

khook_list_t *
simplehook_create(int ipl, const char *name)
{

	(void)ipl;
	(void)name;
	if (khook_next >= KHOOK_LISTS)
		return NULL;
	return &khook_lists[khook_next++];
}

void
simplehook_destroy(khook_list_t *khl)
{

	(void)khl;
}

int
simplehook_dohooks(khook_list_t *khl)
{

	(void)khl;
	return 0;
}

khook_t *
simplehook_establish(khook_list_t *khl, void (*fn)(void *), void *arg)
{

	(void)khl;
	(void)fn;
	(void)arg;
	return NULL;
}

void
simplehook_disestablish(khook_list_t *khl, khook_t *hk, kmutex_t *mp)
{

	(void)khl;
	(void)hk;
	(void)mp;
}

bool
simplehook_has_hooks(khook_list_t *khl)
{

	(void)khl;
	return false;
}
