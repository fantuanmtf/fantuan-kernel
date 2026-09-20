/* rump_ether_if.c - shared ethernet ifnet core for the NIC drivers (ours,
 * M11 R9b, lifted from the R6 e1000 adapter).  This is a minimal in-tree
 * counterpart of if_ethersubr.c's ether_output (not imported): AF_INET
 * frames go through the real arpresolve(), AF_ARP frames take the target
 * hardware address, both get the Ethernet header prepended and are queued
 * through the real ifq_enqueue()/if_start path.  Receive drains the driver's
 * polled ring, wraps each frame in a cluster mbuf and feeds it to
 * ip_pktq/arp_pktq exactly where ether_input would demux the ethertype.
 * if_csum_flags_{tx,rx} stay 0 so the stack computes/verifies checksums in
 * software.  The driver supplies only struct rump_ether_hw. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include <sys/mbuf.h>
#include <sys/socket.h>
#include <net/if.h>
#include <net/if_types.h>
#include <net/if_dl.h>
#include <net/if_arp.h>
#include <net/if_ether.h>
#include <net/if_llatbl.h>
#include <net/if_stats.h>
#include <net/pktqueue.h>
#include <net/route.h>
#include <netinet/in.h>
#include <netinet/in_systm.h>
#include <netinet/in_var.h>
#include <netinet/if_inarp.h>
#include <netinet/ip.h>
#include <netinet/ip_var.h>
#include "rump_shim.h"
#include "rump_ether_if.h"

/* if_ethersubr.c (not imported) normally defines this. */
const uint8_t etherbroadcastaddr[ETHER_ADDR_LEN] =
    { 0xff, 0xff, 0xff, 0xff, 0xff, 0xff };

static const struct rump_ether_hw *ether_hw;
static const char *ether_name;
static struct ifnet *ether_ifp;
static int ether_ready_flag;

static int
ether_initaddr(struct ifnet *ifp, struct ifaddr *ifa, bool src)
{

	(void)src;
	arp_ifinit(ifp, ifa);
	return 0;
}

static int
ether_output(struct ifnet *ifp, struct mbuf *m, const struct sockaddr *dst,
    const struct rtentry *rt)
{
	uint8_t edst[ETHER_ADDR_LEN];
	struct ether_header *eh;
	uint16_t etype;
	struct arphdr *ah;
	int error;

	if ((ifp->if_flags & (IFF_UP | IFF_RUNNING)) !=
	    (IFF_UP | IFF_RUNNING)) {
		m_freem(m);
		return ENETDOWN;
	}
	switch (dst->sa_family) {
	case AF_INET:
		if (m->m_flags & M_BCAST)
			memcpy(edst, etherbroadcastaddr, sizeof(edst));
		else if (m->m_flags & M_MCAST)
			ETHER_MAP_IP_MULTICAST(&satocsin(dst)->sin_addr,
			    edst);
		else {
			error = arpresolve(ifp, rt, m, dst, edst,
			    sizeof(edst));
			if (error != 0)
				return (error == EWOULDBLOCK) ? 0 : error;
		}
		etype = htons(ETHERTYPE_IP);
		break;
	case AF_ARP:
		ah = mtod(m, struct arphdr *);
		if (m->m_flags & M_BCAST)
			memcpy(edst, etherbroadcastaddr, sizeof(edst));
		else if (ar_tha(ah) != NULL)
			memcpy(edst, ar_tha(ah), sizeof(edst));
		else {
			m_freem(m);
			return 0;
		}
		ah->ar_hrd = htons(ARPHRD_ETHER);
		etype = htons(ETHERTYPE_ARP);
		break;
	default:
		m_freem(m);
		return EAFNOSUPPORT;
	}
	M_PREPEND(m, ETHER_HDR_LEN, M_DONTWAIT);
	if (m == NULL)
		return ENOBUFS;
	eh = mtod(m, struct ether_header *);
	eh->ether_type = etype;
	memcpy(eh->ether_dhost, edst, ETHER_ADDR_LEN);
	memcpy(eh->ether_shost, CLLADDR(ifp->if_sadl), ETHER_ADDR_LEN);
	return ifq_enqueue(ifp, m);
}

static void
ether_start(struct ifnet *ifp)
{
	static uint8_t frame[RUMP_ETHER_BUF_SIZE];
	struct mbuf *m;

	while (!IFQ_IS_EMPTY(&ifp->if_snd)) {
		IFQ_DEQUEUE(&ifp->if_snd, m);
		if (m == NULL)
			break;
		if (m->m_pkthdr.len > (int)sizeof(frame)) {
			if_statinc(ifp, if_oerrors);
			m_freem(m);
			continue;
		}
		/* mbufs may be chained (e.g. tcp_output's header); linearize. */
		m_copydata(m, 0, m->m_pkthdr.len, frame);
		if (ether_hw->send(frame, m->m_pkthdr.len) < 0) {
			if_statinc(ifp, if_oerrors);
			m_freem(m);
			continue;
		}
		if_statinc(ifp, if_opackets);
		m_freem(m);
	}
}

static void
ether_demux(const uint8_t *frame, int len)
{
	const struct ether_header *eh;
	struct mbuf *m;
	int plen;

	if (len < ETHER_HDR_LEN)
		return;
	eh = (const struct ether_header *)frame;
	plen = len - ETHER_HDR_LEN;
	m = m_getcl(M_DONTWAIT, MT_DATA, M_PKTHDR);
	if (m == NULL) {
		if_statinc(ether_ifp, if_iqdrops);
		return;
	}
	memcpy(mtod(m, void *), frame + ETHER_HDR_LEN, (size_t)plen);
	m->m_len = m->m_pkthdr.len = plen;
	m_set_rcvif(m, ether_ifp);
	if (memcmp(eh->ether_dhost, etherbroadcastaddr, ETHER_ADDR_LEN) == 0)
		m->m_flags |= M_BCAST;
	else if (ETHER_IS_MULTICAST(eh->ether_dhost))
		m->m_flags |= M_MCAST;
	if_statinc(ether_ifp, if_ipackets);
	if_statadd(ether_ifp, if_ibytes, (uint64_t)len);
	switch (ntohs(eh->ether_type)) {
	case ETHERTYPE_IP:
		if (!pktq_enqueue(ip_pktq, m, 0))
			m_freem(m);
		break;
	case ETHERTYPE_ARP:
		if (!pktq_enqueue(arp_pktq, m, 0))
			m_freem(m);
		break;
	default:
		m_freem(m);
		break;
	}
}

void
rump_ether_poll(void)
{
	uint8_t frame[RUMP_ETHER_BUF_SIZE];
	int len;

	if (!ether_ready_flag)
		return;
	ether_hw->tx_reclaim();
	while ((len = ether_hw->recv(frame, sizeof(frame))) > 0)
		ether_demux(frame, len);
}

/* The driver supplies the hardware ops; the net_ops table lives in
 * rump_ether_ops.c (size rule). */
const struct rump_ether_hw *
rump_ether_hw(void)
{

	return ether_hw;
}

int
rump_ether_up(const char *name, const struct rump_ether_hw *hw)
{
	uint8_t mac[ETHER_ADDR_LEN];
	int error;

	error = hw->init();
	if (error == ENXIO)
		return ENXIO;		/* no device: not a failure */
	if (error != 0) {
		printf("net: nic FAILED (hw)\n");
		return error;
	}
	ether_hw = hw;
	ether_name = name;
	ether_ifp = if_alloc(IFT_ETHER);
	if (ether_ifp == NULL) {
		printf("net: nic FAILED (ifnet)\n");
		return ENOMEM;
	}
	if_initname(ether_ifp, name, 0);
	ether_ifp->if_mtu = ETHERMTU;
	ether_ifp->if_flags = IFF_UP | IFF_BROADCAST | IFF_MULTICAST;
	ether_ifp->if_output = ether_output;
	ether_ifp->if_start = ether_start;
	ether_ifp->if_initaddr = ether_initaddr;
	ether_ifp->if_hdrlen = ETHER_HDR_LEN;
	ether_ifp->if_addrlen = ETHER_ADDR_LEN;
	ether_ifp->if_dlt = DLT_EN10MB;
	if (ether_ifp->if_baudrate == 0)
		ether_ifp->if_baudrate = IF_Mbps(1000);
	ether_ifp->if_csum_flags_tx = 0;
	ether_ifp->if_csum_flags_rx = 0;
	if_initialize(ether_ifp);
	ether_ifp->if_link_state = LINK_STATE_UP;
	ether_hw->mac(mac);
	if_set_sadl(ether_ifp, mac, ETHER_ADDR_LEN, true);
	ether_ifp->if_broadcastaddr = etherbroadcastaddr;
	ether_ifp->if_flags |= IFF_RUNNING;
	if_register(ether_ifp);
	if (net_register(rump_ether_ops(), NULL) < 0 ||
	    net_ifattach(rump_ether_ops(), ether_ifp) != 0) {
		printf("net: nic FAILED (attach)\n");
		return ENXIO;
	}
	ether_ready_flag = 1;
	printf("net: %s up mac=%02x:%02x:%02x:%02x:%02x:%02x\n",
	    ether_name, mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
	return 0;
}

int
rump_ether_ready(void)
{

	return ether_ready_flag;
}

struct ifnet *
rump_ether_ifp(void)
{

	return ether_ifp;
}

void
rump_ether_mac(uint8_t out[6])
{

	if (ether_hw != NULL)
		ether_hw->mac(out);
}

void
rump_ether_counters(unsigned long long *in, unsigned long long *out)
{
	struct if_data ifd;

	if (ether_ifp == NULL) {
		*in = *out = 0;
		return;
	}
	if_stats_to_if_data(ether_ifp, &ifd, false);
	*in = (unsigned long long)ifd.ifi_ipackets;
	*out = (unsigned long long)ifd.ifi_opackets;
}
