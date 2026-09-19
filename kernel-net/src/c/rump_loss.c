/* rump_loss.c - deterministic loopback loss injection for the R5 retransmit
 * test (ours).  pktq_enqueue() consults this hook before queueing an IP
 * packet: while armed, the first ndrops data segments of one TCP direction
 * are freed while the interface counters stay as if they had been
 * transmitted (the loss is "on the wire"), and the sequence ranges are kept
 * so the retransmissions that recover them can be counted. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include <sys/mbuf.h>
#include <net/if.h>
#include <net/pktqueue.h>
#include <netinet/in.h>
#include <netinet/in_var.h>
#include <netinet/ip.h>
#include <netinet/tcp.h>
#include "rump_shim.h"

#define LOSS_KEYMAX 16

static struct {
	uint32_t lk_seq;
} loss_keys[LOSS_KEYMAX];
static int loss_nkeys, loss_drop_left;
static uint16_t loss_sport, loss_dport;
static unsigned loss_dropped, loss_retrans;

void
rump_loss_arm(uint16_t sport, uint16_t dport, int ndrops)
{

	loss_nkeys = 0;
	loss_drop_left = ndrops;
	loss_sport = sport;
	loss_dport = dport;
	loss_dropped = 0;
	loss_retrans = 0;
}

void
rump_loss_disarm(void)
{

	loss_drop_left = 0;
	loss_nkeys = 0;
}

unsigned
rump_loss_dropped(void)
{

	return loss_dropped;
}

unsigned
rump_loss_retrans(void)
{

	return loss_retrans;
}

/* Decides whether this packet is a dropped segment or a retransmission of
 * one.  Only TCP segments of the armed direction are considered. */
static bool
loss_filter(struct mbuf *m)
{
	struct ip ip;
	struct tcphdr th;
	size_t ihl, plen;
	uint32_t seq;
	int i;

	if ((m->m_flags & M_PKTHDR) == 0 || m->m_len < (int)sizeof(ip))
		return false;
	memcpy(&ip, mtod(m, const void *), sizeof(ip));
	if (ip.ip_v != 4 || ip.ip_p != IPPROTO_TCP)
		return false;
	ihl = (size_t)ip.ip_hl << 2;
	if (ihl + sizeof(th) > (size_t)m->m_len)
		return false;
	memcpy(&th, (const char *)mtod(m, const void *) + ihl, sizeof(th));
	if (th.th_sport != htons(loss_sport) ||
	    th.th_dport != htons(loss_dport))
		return false;
	seq = ntohl(th.th_seq);
	plen = m->m_pkthdr.len - ihl - ((size_t)th.th_off << 2);

	/* First, does this segment cover sequence space we dropped?  A dropped
	 * range can only come back on a retransmission, and one retransmitted
	 * segment may cover several dropped ones after re-segmentation. */
	for (i = 0; i < loss_nkeys; ) {
		int32_t off = (int32_t)(loss_keys[i].lk_seq - seq);

		if (off >= 0 && off < (int32_t)plen) {
			loss_keys[i] = loss_keys[--loss_nkeys];
			loss_retrans++;
			continue;
		}
		i++;
	}
	if (loss_drop_left > 0 && plen > 0 &&
	    (th.th_flags & (TH_SYN|TH_FIN|TH_RST)) == 0) {
		loss_drop_left--;
		loss_dropped++;
		if (loss_nkeys < LOSS_KEYMAX)
			loss_keys[loss_nkeys].lk_seq = seq;
		loss_nkeys++;
		return true;
	}
	return false;
}

bool
rump_loss_drop_if_armed(pktqueue_t *pq, struct mbuf *m)
{

	if (pq != ip_pktq || (loss_drop_left == 0 && loss_nkeys == 0))
		return false;
	return loss_filter(m);
}
