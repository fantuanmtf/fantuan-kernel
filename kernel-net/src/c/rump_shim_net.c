/* rump_shim_net.c - net_ops registry, packet queue and domain helpers
 * (ours).  The ifnet core is the imported NetBSD if.c; this file owns the
 * fantuan-side driver registry (docs/M11_NET.md section 6), the single-CPU
 * mbuf packet queue that stands in for pktqueue(9) until the softint-driven
 * IP input path lands (R4), and the mbuf tunables.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/mbuf.h>
#include <sys/domain.h>
#include <sys/protosw.h>
#include <sys/pslist.h>
#include <sys/psref.h>
#include <sys/lwp.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <net/if.h>
#include <net/pktqueue.h>
#include "rump_shim.h"

struct domainhead domains = STAILQ_HEAD_INITIALIZER(domains);

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
	int error;

	if (nd == NULL)
		return -1;
	nd->nd_ifp = ifp;
	nd->nd_priv = ifp;
	if (ops->init != NULL) {
		error = ops->init(ifp);
		if (error != 0)
			return error;
	}
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

/* pktqueue(9) stand-in: one queue drained by the loopback task.  The
 * struct is opaque in pktqueue.h (its owner is pktqueue.c, not imported). */
struct pktqueue {
	struct mbuf *pq_head;
	struct mbuf *pq_tail;
	int pq_len;
};

static struct pktqueue ip_pktq_store;
pktqueue_t *ip_pktq = &ip_pktq_store;

void
rump_netq_enqueue(struct mbuf *m)
{
	int s = splnet();

	m->m_nextpkt = NULL;
	if (ip_pktq->pq_tail != NULL)
		ip_pktq->pq_tail->m_nextpkt = m;
	else
		ip_pktq->pq_head = m;
	ip_pktq->pq_tail = m;
	ip_pktq->pq_len++;
	splx(s);
}

struct mbuf *
rump_netq_dequeue(void)
{
	struct mbuf *m;
	int s = splnet();

	m = ip_pktq->pq_head;
	if (m != NULL) {
		ip_pktq->pq_head = m->m_nextpkt;
		if (ip_pktq->pq_head == NULL)
			ip_pktq->pq_tail = NULL;
		m->m_nextpkt = NULL;
		ip_pktq->pq_len--;
	}
	splx(s);
	return m;
}

bool
pktq_enqueue(pktqueue_t *pq, struct mbuf *m, const u_int flags)
{

	(void)pq;
	(void)flags;
	if (ip_pktq->pq_len >= 64)
		return false;
	rump_netq_enqueue(m);
	return true;
}

void
pktq_ifdetach(void)
{
}

struct domain *
pffinddomain(int family)
{
	struct domain *dp;

	DOMAIN_FOREACH(dp) {
		if (dp->dom_family == family)
			return dp;
	}
	return NULL;
}

void
pfctlinput(int cmd, const struct sockaddr *sa)
{
	struct domain *dp;
	const struct protosw *pr;

	DOMAIN_FOREACH(dp) {
		for (pr = dp->dom_protosw; pr < dp->dom_protoswNPROTOSW; pr++) {
			if (pr->pr_ctlinput != NULL)
				(void)pr->pr_ctlinput(cmd, sa, NULL);
		}
	}
}


