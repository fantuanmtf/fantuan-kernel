/* rump_ping.c - ICMP echo over loopback, the R3 stand-in for the real
 * ip_input.c/ip_icmp.c path (ours).  rump_net_poll() drains the packet
 * queue: an echo request is turned around in place (addresses swapped,
 * type 8 -> 0, checksums redone) and fed back through if_output; the echo
 * reply is handed to the ping client's recv op and matched by id/seq.
 * Timing uses the PIT tick (100 Hz).  Bounded retries, then quiet.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/kernel.h>
#include <sys/mbuf.h>
#include <sys/socket.h>
#include <net/if.h>
#include <net/if_stats.h>
#include <netinet/in.h>
#include <netinet/in_systm.h>
#include <netinet/in_var.h>
#include <netinet/ip.h>
#include "rump_shim.h"

#define PING_IDENT	0x4c54
#define PING_PAYLOAD	8
#define PING_TRIES	3
#define PING_TIMEOUT	50	/* PIT ticks (100 Hz): 0.5 s per try */

struct lo_icmp {
	uint8_t		type;
	uint8_t		code;
	uint16_t	cksum;
	uint16_t	id;
	uint16_t	seq;
};

static int ping_phase;
static int ping_try;
static int ping_seq = 1;
static int ping_sent_ticks;

static uint16_t
lo_cksum(const void *data, size_t len)
{
	const uint8_t *p = data;
	uint32_t sum = 0;
	size_t i;

	for (i = 0; i + 1 < len; i += 2)
		sum += ((uint16_t)p[i] << 8) | p[i + 1];
	if (i < len)
		sum += (uint16_t)p[i] << 8;
	while (sum >> 16)
		sum = (sum & 0xffff) + (sum >> 16);
	return (uint16_t)~sum;
}

static void
lo_set_sockaddr(struct sockaddr_in *sin)
{

	memset(sin, 0, sizeof(*sin));
	sin->sin_len = sizeof(*sin);
	sin->sin_family = AF_INET;
	sin->sin_addr.s_addr = htonl(INADDR_LOOPBACK);
}

static void
lo_fail(const char *step)
{

	printf("net: loopback FAILED (%s)\n", step);
}

static void
lo_input(struct mbuf *m)
{
	struct ip *ip = mtod(m, struct ip *);
	struct lo_icmp *ic;
	size_t iplen, icmplen;
	struct sockaddr_in dst;

	if (m->m_len < (int)sizeof(*ip) || ip->ip_v != 4 ||
	    ip->ip_p != IPPROTO_ICMP)
		goto drop;
	iplen = (size_t)ip->ip_hl << 2;
	if (iplen < sizeof(*ip) || (size_t)m->m_len < iplen + sizeof(*ic))
		goto drop;
	ic = (struct lo_icmp *)((uint8_t *)ip + iplen);
	icmplen = (size_t)m->m_len - iplen;

	if (ic->type == 8 /* ICMP_ECHO */) {
		struct in_addr src = ip->ip_src;

		ip->ip_src = ip->ip_dst;
		ip->ip_dst = src;
		ic->type = 0;	/* ICMP_ECHOREPLY */
		ic->code = 0;
		ic->cksum = 0;
		ic->cksum = lo_cksum(ic, icmplen);
		ip->ip_sum = 0;
		ip->ip_sum = lo_cksum(ip, iplen);
		lo_set_sockaddr(&dst);
		(void)rump_loopback_ifp()->if_output(rump_loopback_ifp(), m,
		    (struct sockaddr *)&dst, NULL);
		return;
	}
	if (ic->type == 0 /* ICMP_ECHOREPLY */) {
		rump_loopback_rx(m);
		return;
	}
drop:
	m_freem(m);
}

static int
lo_ping_send(void)
{
	uint8_t pkt[64];
	struct ip *ip = (struct ip *)pkt;
	struct lo_icmp *ic;
	size_t iplen = sizeof(*ip);
	size_t len = iplen + sizeof(*ic) + PING_PAYLOAD;
	int stamp = getticks();

	memset(pkt, 0, sizeof(pkt));
	ip->ip_v = 4;
	ip->ip_hl = 5;
	ip->ip_len = htons((uint16_t)len);
	ip->ip_id = htons((uint16_t)ping_seq);
	ip->ip_ttl = 64;
	ip->ip_p = IPPROTO_ICMP;
	ip->ip_src.s_addr = htonl(INADDR_LOOPBACK);
	ip->ip_dst.s_addr = htonl(INADDR_LOOPBACK);
	ic = (struct lo_icmp *)(pkt + iplen);
	ic->type = 8;
	ic->code = 0;
	ic->id = htons(PING_IDENT);
	ic->seq = htons((uint16_t)ping_seq);
	memcpy(pkt + iplen + sizeof(*ic), &stamp, sizeof(stamp));
	ic->cksum = lo_cksum(ic, len - iplen);
	ip->ip_sum = lo_cksum(ip, iplen);
	ping_seq++;
	ping_try++;
	ping_sent_ticks = getticks();
	return net_send(pkt, len) == 0;
}

static int
lo_reply_matches(const uint8_t *buf, int n)
{
	const struct ip *ip = (const struct ip *)buf;
	const struct lo_icmp *ic;
	size_t iplen;

	if (n < (int)sizeof(*ip) || ip->ip_v != 4 ||
	    ip->ip_p != IPPROTO_ICMP)
		return 0;
	iplen = (size_t)ip->ip_hl << 2;
	if (n < (int)(iplen + sizeof(*ic)))
		return 0;
	ic = (const struct lo_icmp *)(buf + iplen);
	return ic->type == 0 && ntohs(ic->id) == PING_IDENT &&
	    ntohs(ic->seq) == (uint16_t)(ping_seq - 1);
}

static void
lo_success(void)
{
	struct if_data ifd;
	int rtt = getticks() - ping_sent_ticks;

	if (rtt < 1)
		rtt = 1;
	if_stats_to_if_data(rump_loopback_ifp(), &ifd, false);
	printf("net: ping 127.0.0.1 ok (seq=%d rtt=%d ticks)\n", ping_seq - 1,
	    rtt);
	printf("net: icmp echo reply ok\n");
	printf("net: in/out counters pkts_in=%llu pkts_out=%llu\n",
	    (unsigned long long)ifd.ifi_ipackets,
	    (unsigned long long)ifd.ifi_opackets);
}

int
rump_net_poll(void)
{
	struct mbuf *m;
	uint8_t buf[64];
	int n, i;

	for (i = 0; i < 8; i++) {
		m = rump_netq_dequeue();
		if (m == NULL)
			break;
		lo_input(m);
	}

	if (!rump_loopback_ready())
		return 1;

	switch (ping_phase) {
	case 0:
		if (!lo_ping_send()) {
			lo_fail("send");
			ping_phase = 3;
			return 0;
		}
		ping_phase = 1;
		break;
	case 1:
		n = net_recv(buf, sizeof(buf));
		if (n > 0 && lo_reply_matches(buf, n)) {
			lo_success();
			ping_phase = 3;
			return 0;
		}
		if (getticks() - ping_sent_ticks > PING_TIMEOUT) {
			if (ping_try >= PING_TRIES) {
				lo_fail("ping");
				ping_phase = 3;
				return 0;
			}
			if (!lo_ping_send()) {
				lo_fail("send");
				ping_phase = 3;
				return 0;
			}
		}
		break;
	case 3:
		return 1;
	default:
		break;
	}
	return 0;
}
