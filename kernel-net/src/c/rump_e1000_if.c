/* rump_e1000_if.c - e1000 ifnet, net_ops table and RX demux (ours).
 * This is a minimal in-tree counterpart of if_ethersubr.c's ether_output
 * (which is not part of the imported slice): AF_INET frames go through the
 * real arpresolve(), AF_ARP frames take the target hardware address, both
 * get the Ethernet header prepended and are queued through the real
 * ifq_enqueue()/if_start path.  Receive drains the polled RX ring, wraps
 * each frame in a cluster mbuf and feeds it to ip_pktq/arp_pktq exactly
 * where ether_input would demux the ethertype.  if_csum_flags_{tx,rx} stay
 * 0 so the stack computes/verifies checksums in software. */

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
#include "rump_e1000.h"

/* if_ethersubr.c (not imported) normally defines this. */
const uint8_t etherbroadcastaddr[ETHER_ADDR_LEN] =
    { 0xff, 0xff, 0xff, 0xff, 0xff, 0xff };

static struct ifnet *e1000_ifp;
static int e1000_ready_flag;

static int
e1000_initaddr(struct ifnet *ifp, struct ifaddr *ifa, bool src)
{

	(void)src;
	arp_ifinit(ifp, ifa);
	return 0;
}

static int
e1000_output(struct ifnet *ifp, struct mbuf *m, const struct sockaddr *dst,
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
e1000_start(struct ifnet *ifp)
{
	static uint8_t frame[E1000_BUF_SIZE];
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
		if (e1000_hw_send(frame, m->m_pkthdr.len) < 0) {
			if_statinc(ifp, if_oerrors);
			m_freem(m);
			continue;
		}
		if_statinc(ifp, if_opackets);
		m_freem(m);
	}
}

static void
e1000_demux(const uint8_t *frame, int len)
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
		if_statinc(e1000_ifp, if_iqdrops);
		return;
	}
	memcpy(mtod(m, void *), frame + ETHER_HDR_LEN, (size_t)plen);
	m->m_len = m->m_pkthdr.len = plen;
	m_set_rcvif(m, e1000_ifp);
	if (memcmp(eh->ether_dhost, etherbroadcastaddr, ETHER_ADDR_LEN) == 0)
		m->m_flags |= M_BCAST;
	else if (ETHER_IS_MULTICAST(eh->ether_dhost))
		m->m_flags |= M_MCAST;
	if_statinc(e1000_ifp, if_ipackets);
	if_statadd(e1000_ifp, if_ibytes, (uint64_t)len);
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
rump_e1000_poll(void)
{
	uint8_t frame[E1000_BUF_SIZE];
	int len;

	if (!e1000_ready_flag)
		return;
	e1000_hw_tx_reclaim();
	while ((len = e1000_hw_recv(frame, sizeof(frame))) > 0)
		e1000_demux(frame, len);
}

int
rump_e1000_up(void)
{
	uint8_t mac[ETHER_ADDR_LEN];
	int error;

	error = e1000_hw_init();
	if (error == ENXIO)
		return ENXIO;		/* no device: not a failure */
	if (error != 0) {
		printf("net: nic FAILED (hw)\n");
		return error;
	}
	e1000_ifp = if_alloc(IFT_ETHER);
	if (e1000_ifp == NULL) {
		printf("net: nic FAILED (ifnet)\n");
		return ENOMEM;
	}
	if_initname(e1000_ifp, "e1000", 0);
	e1000_ifp->if_mtu = ETHERMTU;
	e1000_ifp->if_flags = IFF_UP | IFF_BROADCAST | IFF_MULTICAST;
	e1000_ifp->if_output = e1000_output;
	e1000_ifp->if_start = e1000_start;
	e1000_ifp->if_initaddr = e1000_initaddr;
	e1000_ifp->if_hdrlen = ETHER_HDR_LEN;
	e1000_ifp->if_addrlen = ETHER_ADDR_LEN;
	e1000_ifp->if_dlt = DLT_EN10MB;
	if (e1000_ifp->if_baudrate == 0)
		e1000_ifp->if_baudrate = IF_Mbps(1000);
	e1000_ifp->if_csum_flags_tx = 0;
	e1000_ifp->if_csum_flags_rx = 0;
	if_initialize(e1000_ifp);
	e1000_ifp->if_link_state = LINK_STATE_UP;
	e1000_hw_mac(mac);
	if_set_sadl(e1000_ifp, mac, ETHER_ADDR_LEN, true);
	e1000_ifp->if_broadcastaddr = etherbroadcastaddr;
	e1000_ifp->if_flags |= IFF_RUNNING;
	if_register(e1000_ifp);
	if (net_register(&e1000_net_ops, NULL) < 0 ||
	    net_ifattach(&e1000_net_ops, e1000_ifp) != 0) {
		printf("net: nic FAILED (attach)\n");
		return ENXIO;
	}
	e1000_ready_flag = 1;
	printf("net: e1000 up mac=%02x:%02x:%02x:%02x:%02x:%02x\n",
	    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
	return 0;
}

int
rump_e1000_ready(void)
{

	return e1000_ready_flag;
}

struct ifnet *
rump_e1000_ifp(void)
{

	return e1000_ifp;
}

void
rump_e1000_counters(unsigned long long *in, unsigned long long *out)
{
	struct if_data ifd;

	if (e1000_ifp == NULL) {
		*in = *out = 0;
		return;
	}
	if_stats_to_if_data(e1000_ifp, &ifd, false);
	*in = (unsigned long long)ifd.ifi_ipackets;
	*out = (unsigned long long)ifd.ifi_opackets;
}
