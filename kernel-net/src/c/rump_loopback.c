/* rump_loopback.c - lo0 bring-up and net_ops glue (ours).
 * lo0 itself is the imported NetBSD if_loop.c driver, attached to the real
 * if.c ifnet core.  This file assigns 127.0.0.1/8, registers the loopback
 * net_ops (docs/M11_NET.md section 6) and owns the receive queue the echo
 * responder hands finished replies to.  The ping/ICMP logic lives in
 * rump_ping.c; R4 replaces both with ip_input.c/ip_icmp.c/in.c.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/mbuf.h>
#include <sys/socket.h>
#include <sys/ioctl.h>
#include <sys/malloc.h>
#include <net/if.h>
#include <net/bpf.h>
#include <net/route.h>
#include <netinet/in.h>
#include <netinet/in_systm.h>
#include <netinet/in_var.h>
#include <netinet/ip.h>
#include "rump_shim.h"

extern struct ifnet *lo0ifp;
void loopattach(int);
void rt_init(void);

static int lo_ready;
static struct mbuf *lo_rxq_head;
static struct mbuf *lo_rxq_tail;

static void
lo_set_sockaddr(struct sockaddr_in *sin)
{

	memset(sin, 0, sizeof(*sin));
	sin->sin_len = sizeof(*sin);
	sin->sin_family = AF_INET;
	sin->sin_addr.s_addr = htonl(INADDR_LOOPBACK);
}

static int
loop_send(void *priv, const void *frame, size_t len)
{
	struct ifnet *ifp = priv;
	struct sockaddr_in dst;
	struct mbuf *m;

	if (len > MHLEN)
		return -1;
	m = m_gethdr(M_DONTWAIT, MT_DATA);
	if (m == NULL)
		return -1;
	memcpy(mtod(m, void *), frame, len);
	m->m_len = m->m_pkthdr.len = (int)len;
	lo_set_sockaddr(&dst);
	return ifp->if_output(ifp, m, (struct sockaddr *)&dst, NULL);
}

static int
loop_recv(void *priv, void *frame, size_t max)
{
	struct mbuf *m;
	size_t n;
	int s;

	(void)priv;
	s = splnet();
	m = lo_rxq_head;
	if (m != NULL) {
		lo_rxq_head = m->m_nextpkt;
		if (lo_rxq_head == NULL)
			lo_rxq_tail = NULL;
		m->m_nextpkt = NULL;
	}
	splx(s);
	if (m == NULL)
		return 0;
	n = (size_t)m->m_len;
	if (n > max)
		n = max;
	memcpy(frame, mtod(m, void *), n);
	m_freem(m);
	return (int)n;
}

static void
loop_mac(void *priv, uint8_t out[6])
{

	(void)priv;
	memset(out, 0, 6);
}

static int
loop_link(void *priv)
{
	struct ifnet *ifp = priv;

	return (ifp->if_flags & IFF_UP) != 0;
}

static int
loop_init(void *priv)
{
	struct ifnet *ifp = priv;

	if (ifp != lo0ifp || (ifp->if_flags & IFF_RUNNING) == 0)
		return -1;
	return 0;
}

static const struct net_ops loop_ops = {
	.name = "loopback",
	.init = loop_init,
	.mac = loop_mac,
	.send = loop_send,
	.recv = loop_recv,
	.link = loop_link,
};

static void
lo_setaddr(void)
{
	struct in_ifaddr *ia;
	struct ifaddr *ifa;

	ia = malloc(sizeof(*ia), M_IFADDR, M_WAITOK | M_ZERO);
	ia->ia_addr.sin_family = AF_INET;
	ia->ia_addr.sin_len = sizeof(ia->ia_addr);
	ia->ia_addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
	ia->ia_dstaddr = ia->ia_addr;
	ia->ia_subnetmask = htonl(IN_CLASSA_NET);
	ia->ia_netmask = ia->ia_subnetmask;
	ia->ia_sockmask.sin_family = AF_INET;
	ia->ia_sockmask.sin_len = sizeof(ia->ia_sockmask);
	ia->ia_sockmask.sin_addr.s_addr = ia->ia_subnetmask;
	ifa = &ia->ia_ifa;
	ifa->ifa_addr = (struct sockaddr *)&ia->ia_addr;
	ifa->ifa_dstaddr = (struct sockaddr *)&ia->ia_dstaddr;
	ifa->ifa_netmask = (struct sockaddr *)&ia->ia_sockmask;
	ifa_psref_init(ifa);
	ifa_insert(lo0ifp, ifa);
	if (lo0ifp->if_ioctl(lo0ifp, SIOCINITIFADDR, ifa) != 0 ||
	    (lo0ifp->if_flags & IFF_UP) == 0)
		printf("net: loopback FAILED (addr)\n");
}

void
rump_loopback_rx(struct mbuf *m)
{
	int s = splnet();

	m->m_nextpkt = NULL;
	if (lo_rxq_tail != NULL)
		lo_rxq_tail->m_nextpkt = m;
	else
		lo_rxq_head = m;
	lo_rxq_tail = m;
	splx(s);
}

struct ifnet *
rump_loopback_ifp(void)
{

	return lo0ifp;
}

int
rump_loopback_ready(void)
{

	return lo_ready;
}

void
rump_loopback_up(void)
{

	rt_init();
	bpf_setops();
	ifinit1();
	ifinit();
	loopattach(0);
	if (lo0ifp == NULL) {
		printf("net: loopback FAILED (bring-up)\n");
		return;
	}
	lo_setaddr();
	if (net_register(&loop_ops, NULL) < 0 ||
	    net_ifattach(&loop_ops, lo0ifp) != 0) {
		printf("net: loopback FAILED (attach)\n");
		return;
	}
	lo_ready = 1;
	printf("net: lo0 up 127.0.0.1/8\n");
}
