/* rump_shim_net.c - net_ops registry, pktqueue and mbuf tunables (ours).
 * The ifnet core, the IPv4 stack and the route table are imported NetBSD
 * files; this file owns the driver registry (docs/M11_NET.md section 6) and
 * the single-CPU pktqueue(9) replacement (the real pktqueue.c schedules
 * per-CPU softints over pcq; the adapter keeps one mbuf list per queue and
 * drains it from the net task).  The domain list/lookups are in
 * rump_domain.c. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/kmem.h>
#include <sys/mbuf.h>
#include <sys/domain.h>
#include <sys/protosw.h>
#include <sys/mutex.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <net/if.h>
#include <net/pktqueue.h>
#include "rump_shim.h"

const int msize = MSIZE;
const int mclbytes = MCLBYTES;
int nmbclusters;
int mblowat = 1;
int mcllowat = 1;

#define NET_MAX_DEVICES 4

struct netdev {
	const struct net_ops *nd_ops;
	void *nd_priv;
	struct ifnet *nd_ifp;
};

static struct netdev netdevs[NET_MAX_DEVICES];
static int netdev_count;

int
net_register(const struct net_ops *ops, void *priv)
{

	if (ops == NULL || ops->name == NULL || netdev_count >= NET_MAX_DEVICES)
		return -1;
	netdevs[netdev_count].nd_ops = ops;
	netdevs[netdev_count].nd_priv = priv;
	netdevs[netdev_count].nd_ifp = NULL;
	return netdev_count++;
}

static struct netdev *
net_find(const struct net_ops *ops)
{
	int i;

	for (i = 0; i < netdev_count; i++)
		if (netdevs[i].nd_ops == ops)
			return &netdevs[i];
	return NULL;
}

int
net_ifattach(const struct net_ops *ops, struct ifnet *ifp)
{
	struct netdev *nd = net_find(ops);

	if (nd == NULL)
		return -1;
	nd->nd_ifp = ifp;
	nd->nd_priv = ifp;
	return 0;
}

void
net_ifdetach(const struct net_ops *ops)
{
	struct netdev *nd = net_find(ops);

	if (nd != NULL) {
		nd->nd_ifp = NULL;
		nd->nd_priv = NULL;
	}
}

int
net_send(const void *frame, size_t len)
{

	if (netdev_count == 0 || netdevs[0].nd_ops->send == NULL)
		return -1;
	return netdevs[0].nd_ops->send(netdevs[0].nd_priv, frame, len);
}

int
net_recv(void *frame, size_t max)
{

	if (netdev_count == 0 || netdevs[0].nd_ops->recv == NULL)
		return -1;
	return netdevs[0].nd_ops->recv(netdevs[0].nd_priv, frame, max);
}

/* pktqueue(9) replacement: one mbuf list per queue plus the drain callback
 * ip_input.c/if_arp.c register at creation time.  Enqueueing marks the queue
 * scheduled; the net task calls rump_pktq_drain() and the callback loops
 * pktq_dequeue() until the queue is empty (softint semantics, one CPU). */
struct pktqueue {
	struct mbuf *pq_head;
	struct mbuf *pq_tail;
	int pq_len;
	u_int pq_maxlen;
	bool pq_scheduled;
	u_int pq_drops;
	void (*pq_func)(void *);
	void *pq_arg;
	LIST_ENTRY(pktqueue) pq_link;
};

static LIST_HEAD(, pktqueue) pktqueue_list = LIST_HEAD_INITIALIZER(pktqueue_list);

pktqueue_t *
pktq_create(size_t maxlen, void (*func)(void *), void *arg)
{
	pktqueue_t *pq;

	pq = kmem_zalloc(sizeof(*pq), KM_SLEEP);
	if (pq == NULL)
		return NULL;
	pq->pq_maxlen = (u_int)maxlen;
	pq->pq_func = func;
	pq->pq_arg = arg;
	LIST_INSERT_HEAD(&pktqueue_list, pq, pq_link);
	return pq;
}

void
pktq_destroy(pktqueue_t *pq)
{

	pktq_flush(pq);
	LIST_REMOVE(pq, pq_link);
	kmem_free(pq, sizeof(*pq));
}

bool
pktq_enqueue(pktqueue_t *pq, struct mbuf *m, const u_int flags)
{
	int s;

	(void)flags;
	/* rump_loss.c (R5 test hook): free a "lost" segment while the
	 * interface counters still count it as transmitted. */
	if (rump_loss_drop_if_armed(pq, m)) {
		m_freem(m);
		return true;
	}
	s = splnet();
	if (pq->pq_len >= (int)pq->pq_maxlen) {
		pq->pq_drops++;
		splx(s);
		return false;
	}
	m->m_nextpkt = NULL;
	if (pq->pq_tail != NULL)
		pq->pq_tail->m_nextpkt = m;
	else
		pq->pq_head = m;
	pq->pq_tail = m;
	pq->pq_len++;
	pq->pq_scheduled = true;
	splx(s);
	return true;
}

struct mbuf *
pktq_dequeue(pktqueue_t *pq)
{
	struct mbuf *m;
	int s;

	s = splnet();
	m = pq->pq_head;
	if (m != NULL) {
		pq->pq_head = m->m_nextpkt;
		if (pq->pq_head == NULL)
			pq->pq_tail = NULL;
		m->m_nextpkt = NULL;
		pq->pq_len--;
		if (pq->pq_len == 0)
			pq->pq_scheduled = false;
	}
	splx(s);
	return m;
}

void
rump_pktq_drain(void)
{
	pktqueue_t *pq, *next;
	void (*func)(void *);

	for (pq = LIST_FIRST(&pktqueue_list); pq != NULL; pq = next) {
		next = LIST_NEXT(pq, pq_link);
		if (!pq->pq_scheduled || pq->pq_func == NULL)
			continue;
		func = pq->pq_func;
		func(pq->pq_arg);
	}
}

void
pktq_barrier(pktqueue_t *pq)
{
	(void)pq;
}

void
pktq_ifdetach(void) {}	/* no per-CPU queues to detach */

void
pktq_flush(pktqueue_t *pq)
{
	struct mbuf *m;

	while ((m = pktq_dequeue(pq)) != NULL)
		m_freem(m);
}

int
pktq_set_maxlen(pktqueue_t *pq, size_t maxlen)
{

	if (maxlen == 0)
		return EINVAL;
	pq->pq_maxlen = (u_int)maxlen;
	return 0;
}

uint32_t
pktq_rps_hash(const pktq_rps_hash_func_t *hash, const struct mbuf *m)
{

	(void)hash;
	(void)m;
	return 0;
}

const pktq_rps_hash_func_t pktq_rps_hash_default = NULL;

void
pktq_sysctl_setup(pktqueue_t *pq, struct sysctllog **log,
    const struct sysctlnode *node, const int flags)
{
	(void)pq, (void)log, (void)node, (void)flags;
}

/* pffinddomain/pffindtype/pffindproto/pfctlinput and the domain list live
 * in rump_domain.c. */
