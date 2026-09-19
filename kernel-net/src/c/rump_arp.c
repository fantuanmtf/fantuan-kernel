/* rump_arp.c - ARP self-test on a shim ethernet interface (ours).
 * Loopback has no link layer, so ARP is exercised on a minimal IFT_ETHER
 * ifnet: real if.c core, a 02:00:00:00:01:01 MAC, 10.0.0.1/24 configured
 * through the real in.c SIOCAIFADDR path, an ARP lltable from in_domifattach,
 * and if_output as the capture point (there is no NIC to transmit on).
 * A synthetic ARP request for 10.0.0.1 from 10.0.0.2 is enqueued on the real
 * arp_pktq exactly as ether_input would; arpintr -> in_arpinput learns the
 * sender and reflects the request into a reply (captured and checked), then
 * a synthetic reply from the same peer makes the cache entry REACHABLE and
 * arpresolve() returns the peer MAC.  DAD is disabled for the offline lab
 * interface (ip_dad_count = 0) so the address is usable immediately.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/mbuf.h>
#include <sys/socket.h>
#include <sys/ioctl.h>
#include <net/if.h>
#include <net/if_types.h>
#include <net/if_dl.h>
#include <net/if_arp.h>
#include <net/if_ether.h>
#include <net/if_llatbl.h>
#include <netinet/in.h>
#include <netinet/in_systm.h>
#include <netinet/in_var.h>
#include <netinet/if_inarp.h>
#include <netinet/ip.h>
#include "rump_shim.h"

#define ARP_LOCAL_ADDR	0x0a000001	/* 10.0.0.1 */
#define ARP_PEER_ADDR	0x0a000002	/* 10.0.0.2 */

static const uint8_t arp_local_mac[ETHER_ADDR_LEN] =
    { 0x02, 0x00, 0x00, 0x00, 0x01, 0x01 };
static const uint8_t arp_peer_mac[ETHER_ADDR_LEN] =
    { 0x02, 0x00, 0x00, 0x00, 0x02, 0x02 };
static const uint8_t arp_broadcast[ETHER_ADDR_LEN] =
    { 0xff, 0xff, 0xff, 0xff, 0xff, 0xff };

static struct ifnet *arp_ifp;
static uint8_t arp_reply[ETHER_HDR_LEN + sizeof(struct arphdr) +
    ETHER_ADDR_LEN * 2 + sizeof(struct in_addr) * 2];
static int arp_reply_len;
static int arp_frames_out;
static int arp_test_phase;

static int
arp_shim_initaddr(struct ifnet *ifp, struct ifaddr *ifa, bool src)
{

	(void)src;
	arp_ifinit(ifp, ifa);
	return 0;
}

static int
arp_shim_ioctl(struct ifnet *ifp, u_long cmd, void *data)
{

	(void)ifp;
	(void)data;
	switch (cmd) {
	case SIOCADDMULTI:
	case SIOCDELMULTI:
		return 0;
	default:
		return ENOTTY;
	}
}

static int
arp_shim_output(struct ifnet *ifp, struct mbuf *m, const struct sockaddr *dst,
    const struct rtentry *rt)
{
	struct arphdr *ah;

	(void)dst;
	(void)rt;
	if (ifp != NULL && m != NULL &&
	    (size_t)m->m_pkthdr.len >= sizeof(struct arphdr)) {
		ah = mtod(m, struct arphdr *);
		if (ah->ar_hrd == htons(ARPHRD_ETHER) &&
		    ah->ar_pro == htons(ETHERTYPE_IP)) {
			struct ether_header *eh =
			    (struct ether_header *)arp_reply;

			memcpy(eh->ether_dhost, ar_tha(ah), ETHER_ADDR_LEN);
			memcpy(eh->ether_shost, arp_local_mac, ETHER_ADDR_LEN);
			eh->ether_type = htons(ETHERTYPE_ARP);
			arp_reply_len = ETHER_HDR_LEN + m->m_pkthdr.len;
			if (arp_reply_len > (int)sizeof(arp_reply))
				arp_reply_len = sizeof(arp_reply);
			m_copydata(m, 0, arp_reply_len - ETHER_HDR_LEN,
			    arp_reply + ETHER_HDR_LEN);
			arp_frames_out++;
		}
	}
	m_freem(m);
	return 0;
}

static int
arp_shim_up(void)
{
	struct in_aliasreq ifra;

	arp_ifp = if_alloc(IFT_ETHER);
	if (arp_ifp == NULL)
		return -1;
	if_initname(arp_ifp, "shim", 0);
	arp_ifp->if_mtu = ETHERMTU;
	arp_ifp->if_flags = IFF_UP | IFF_BROADCAST | IFF_MULTICAST;
	arp_ifp->if_ioctl = arp_shim_ioctl;
	arp_ifp->if_output = arp_shim_output;
	arp_ifp->if_initaddr = arp_shim_initaddr;
	arp_ifp->if_hdrlen = ETHER_HDR_LEN;
	arp_ifp->if_addrlen = ETHER_ADDR_LEN;
	arp_ifp->if_dlt = DLT_EN10MB;
	if (arp_ifp->if_baudrate == 0)
		arp_ifp->if_baudrate = IF_Mbps(10);
	if_initialize(arp_ifp);
	arp_ifp->if_link_state = LINK_STATE_UP;
	if_set_sadl(arp_ifp, arp_local_mac, ETHER_ADDR_LEN, true);
	arp_ifp->if_broadcastaddr = arp_broadcast;
	arp_ifp->if_flags |= IFF_RUNNING;
	if_register(arp_ifp);

	memset(&ifra, 0, sizeof(ifra));
	strlcpy(ifra.ifra_name, arp_ifp->if_xname, sizeof(ifra.ifra_name));
	ifra.ifra_addr.sin_len = sizeof(ifra.ifra_addr);
	ifra.ifra_addr.sin_family = AF_INET;
	ifra.ifra_addr.sin_addr.s_addr = htonl(ARP_LOCAL_ADDR);
	ifra.ifra_mask.sin_len = sizeof(ifra.ifra_mask);
	ifra.ifra_mask.sin_family = AF_INET;
	ifra.ifra_mask.sin_addr.s_addr = htonl(0xffffff00);
	if (in_control(NULL, SIOCAIFADDR, &ifra, arp_ifp) != 0)
		return -1;
	return 0;
}

static struct mbuf *
arp_frame(const uint8_t *sha, const uint8_t *tha, uint32_t spa, uint32_t tpa,
    uint16_t op)
{
	struct mbuf *m;
	struct arphdr *ah;

	m = m_gethdr(M_DONTWAIT, MT_DATA);
	if (m == NULL)
		return NULL;
	m->m_len = m->m_pkthdr.len = sizeof(*ah) + ETHER_ADDR_LEN * 2 +
	    sizeof(struct in_addr) * 2;
	ah = mtod(m, struct arphdr *);
	ah->ar_hrd = htons(ARPHRD_ETHER);
	ah->ar_pro = htons(ETHERTYPE_IP);
	ah->ar_hln = ETHER_ADDR_LEN;
	ah->ar_pln = sizeof(struct in_addr);
	ah->ar_op = htons(op);
	memcpy(ar_sha(ah), sha, ETHER_ADDR_LEN);
	memcpy(ar_spa(ah), &spa, sizeof(spa));
	memcpy(ar_tha(ah), tha, ETHER_ADDR_LEN);
	memcpy(ar_tpa(ah), &tpa, sizeof(tpa));
	m_set_rcvif(m, arp_ifp);
	return m;
}

void
rump_arp_test_start(void)
{
	struct mbuf *m;

	m = arp_frame(arp_peer_mac, arp_broadcast, htonl(ARP_PEER_ADDR),
	    htonl(ARP_LOCAL_ADDR), ARPOP_REQUEST);
	if (m == NULL) {
		arp_test_phase = 2;
		return;
	}
	m->m_flags |= M_BCAST;
	if (!pktq_enqueue(arp_pktq, m, 0))
		m_freem(m);
	arp_test_phase = 1;
}

static int
arp_reply_check(void)
{
	struct ether_header *eh = (struct ether_header *)arp_reply;
	struct arphdr *ah;
	struct in_addr tpa;

	if (arp_reply_len < (int)sizeof(*eh) + (int)sizeof(*ah))
		return -1;
	if (ntohs(eh->ether_type) != ETHERTYPE_ARP)
		return -1;
	if (memcmp(eh->ether_shost, arp_local_mac, ETHER_ADDR_LEN) != 0)
		return -1;
	ah = (struct arphdr *)(arp_reply + ETHER_HDR_LEN);
	if (ntohs(ah->ar_op) != ARPOP_REPLY)
		return -1;
	if (memcmp(ar_sha(ah), arp_local_mac, ETHER_ADDR_LEN) != 0)
		return -1;
	memcpy(&tpa, ar_tpa(ah), sizeof(tpa));
	if (tpa.s_addr != htonl(ARP_PEER_ADDR))
		return -1;
	return 0;
}

int
rump_arp_test_poll(void)
{
	struct sockaddr_in dst;
	uint8_t desten[ETHER_ADDR_LEN];
	struct mbuf *m;
	int error;

	if (arp_test_phase == 0)
		return 0;
	if (arp_test_phase == 2) {
		printf("net: arp test: frame alloc failed\n");
		return -1;
	}
	if (arp_test_phase == 1) {
		if (arp_reply_check() != 0) {
			printf("net: arp test: reply check failed (len=%d frames=%d)\n",
			    arp_reply_len, arp_frames_out);
			return -1;
		}
		if (LLTABLE(arp_ifp) == NULL ||
		    LLTABLE(arp_ifp)->llt_lle_count < 1) {
			printf("net: arp test: no cache entry (llt=%p)\n",
			    LLTABLE(arp_ifp));
			return -1;
		}
		m = arp_frame(arp_peer_mac, arp_local_mac,
		    htonl(ARP_PEER_ADDR), htonl(ARP_LOCAL_ADDR),
		    ARPOP_REPLY);
		if (m == NULL)
			return -1;
		if (!pktq_enqueue(arp_pktq, m, 0))
			m_freem(m);
		arp_test_phase = 3;
		return 0;
	}

	/* phase 3: the peer entry must now resolve through the real cache. */
	memset(&dst, 0, sizeof(dst));
	dst.sin_len = sizeof(dst);
	dst.sin_family = AF_INET;
	dst.sin_addr.s_addr = htonl(ARP_PEER_ADDR);
	m = m_gethdr(M_DONTWAIT, MT_DATA);
	if (m == NULL)
		return -1;
	m->m_len = m->m_pkthdr.len = 0;
	error = arpresolve(arp_ifp, NULL, m, sintosa(&dst), desten,
	    sizeof(desten));
	if (error != 0) {
		/* arpresolve owns m on every error path (bad: frees it,
		 * nd_resolve() may hold it). */
		printf("net: arp test: arpresolve error=%d\n", error);
		return -1;
	}
	m_freem(m);
	if (memcmp(desten, arp_peer_mac, ETHER_ADDR_LEN) != 0) {
		printf("net: arp test: bad dest mac\n");
		return -1;
	}
	arp_test_phase = 4;
	return 1;
}

int
rump_arp_entries(void)
{

	if (arp_ifp == NULL || LLTABLE(arp_ifp) == NULL)
		return 0;
	return (int)LLTABLE(arp_ifp)->llt_lle_count;
}

int
rump_arp_up(void)
{

	return arp_shim_up();
}
