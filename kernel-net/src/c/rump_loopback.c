/* rump_loopback.c - lo0 bring-up and net_ops glue (ours).
 * lo0 is the imported NetBSD if_loop.c driver attached to the real if.c
 * ifnet core.  This file runs the ifnet init sequence and registers the
 * loopback as the first net_ops device; the IPv4 address is configured
 * through the real in.c control path from rump_ip4.c, and user packets
 * travel ip_output -> looutput -> pktqueue -> ip_input (R4).
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
	memset(&dst, 0, sizeof(dst));
	dst.sin_len = sizeof(dst);
	dst.sin_family = AF_INET;
	dst.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
	return ifp->if_output(ifp, m, (struct sockaddr *)&dst, NULL);
}

static int
loop_recv(void *priv, void *frame, size_t max)
{

	/* Packets arrive through the ifnet input path, not the net_ops
	 * receive hook; driver receive is R6. */
	(void)priv;
	(void)frame;
	(void)max;
	return 0;
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

void
rump_loopback_up(void)
{

	rt_init();
	bpf_setops();
	ifinit1();
	ifinit();
	loopattach(0);
	if (lo0ifp == NULL) {
		printf("net: ip4 FAILED (bring-up)\n");
		return;
	}
	if (net_register(&loop_ops, NULL) < 0 ||
	    net_ifattach(&loop_ops, lo0ifp) != 0) {
		printf("net: ip4 FAILED (attach)\n");
		return;
	}
	lo_ready = 1;
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
