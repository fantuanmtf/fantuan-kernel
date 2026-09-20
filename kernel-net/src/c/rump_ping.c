/* rump_ping.c - ICMP echo client over the real IPv4 path (ours).
 * The request is handed to ip_output(); looutput queues it on ip_pktq and
 * the net task runs ip_input -> icmp_input, which reflects the echo request
 * to an echo reply through icmp_reflect() -> ip_output again.  The reply
 * comes back through ip_input and lands in rip_input(), whose R4 stub hands
 * ICMP echo replies to rump_ping_rx() here.  Bounded retries on the PIT
 * tick; no blocking waits.
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
#include <netinet/ip_icmp.h>
#include <netinet/ip_var.h>
#include "rump_shim.h"

#define PING_IDENT	0x4c54
#define PING_PAYLOAD	8
#define PING_TRIES	3
#define PING_TIMEOUT	50	/* PIT ticks (100 Hz): 0.5 s per try */

static int ping_phase;
static int ping_tries;
static int ping_seq;
static int ping_last_seq;
static int ping_sent_ticks;
static int ping_rtt;
static int ping_reply_ok;
static uint8_t ping_payload[PING_PAYLOAD];
static uint32_t ping_target = 0;	/* network byte order, set on begin */
static const char *ping_label = "127.0.0.1";
static int ping_verbose;		/* print the R4 boot markers */
static const char *ping_step;

const char *
rump_ping_error(void)
{

	return ping_step != NULL ? ping_step : "unknown";
}

int
rump_ping_rtt(void)
{

	return ping_rtt;
}

int
rump_ping_seq(void)
{

	return ping_last_seq;
}

static int
ping_fill(int seq)
{
	struct mbuf *m;
	struct ip *ip;
	struct icmp *ic;
	int iplen = sizeof(struct ip);
	int icmplen = sizeof(struct icmp) + PING_PAYLOAD;
	int total = iplen + icmplen;
	int i;

	m = m_gethdr(M_DONTWAIT, MT_DATA);
	if (m == NULL)
		return -1;
	ip = mtod(m, struct ip *);
	memset(ip, 0, total);
	ip->ip_v = IPVERSION;
	ip->ip_hl = sizeof(struct ip) >> 2;
	ip->ip_len = htons((uint16_t)total);
	ip->ip_id = htons((uint16_t)seq);
	ip->ip_ttl = IPDEFTTL;
	ip->ip_p = IPPROTO_ICMP;
	ip->ip_src.s_addr = 0;
	ip->ip_dst.s_addr = ping_target;
	ic = (struct icmp *)((uint8_t *)ip + iplen);
	ic->icmp_type = ICMP_ECHO;
	ic->icmp_code = 0;
	ic->icmp_id = htons(PING_IDENT);
	ic->icmp_seq = htons((uint16_t)seq);
	for (i = 0; i < PING_PAYLOAD; i++)
		ping_payload[i] = (uint8_t)(seq + i);
	memcpy((uint8_t *)ic + sizeof(struct icmp), ping_payload,
	    PING_PAYLOAD);
	m->m_len = m->m_pkthdr.len = total;
	ic->icmp_cksum = cpu_in_cksum(m, icmplen, iplen, 0);
	ping_last_seq = seq;
	ping_sent_ticks = getticks();
	return ip_output(m, NULL, NULL, 0, NULL, NULL);
}

static int
ping_send(void)
{

	ping_seq++;
	ping_tries++;
	if (ping_fill(ping_seq) != 0) {
		ping_step = "send";
		return -1;
	}
	return 0;
}

static void
ping_report(void)
{
	struct if_data ifd;

	if (!ping_verbose)
		return;
	if_stats_to_if_data(rump_loopback_ifp(), &ifd, false);
	printf("net: ip4 input ok (pkts_in=%llu)\n",
	    (unsigned long long)ifd.ifi_ipackets);
	printf("net: ping %s ok (seq=%d rtt=%d ticks)\n", ping_label,
	    ping_last_seq, ping_rtt);
	printf("net: icmp echo reply ok\n");
}

void
rump_ping_begin(void)
{

	ping_target = htonl(INADDR_LOOPBACK);
	ping_label = "127.0.0.1";
	ping_verbose = 1;
	ping_phase = 1;
	ping_tries = 0;
	ping_seq = 0;
	ping_rtt = 0;
	ping_reply_ok = 0;
	ping_step = NULL;
}

/* M11 R7 tools: echo ADDR (host byte order) without the R4 boot markers;
 * the caller reads rump_ping_seq()/rump_ping_rtt() and prints its line. */
void
rump_ping_begin_addr(uint32_t addr)
{

	ping_target = htonl(addr);
	ping_label = "";
	ping_verbose = 0;
	ping_phase = 1;
	ping_tries = 0;
	ping_seq = 0;
	ping_rtt = 0;
	ping_reply_ok = 0;
	ping_step = NULL;
}

int
rump_ping_poll(void)
{

	if (ping_phase == 0)
		return 0;
	if (ping_phase == 1) {
		if (ping_tries == 0) {
			if (ping_send() != 0)
				return -1;
			return 0;
		}
		if (ping_reply_ok) {
			ping_rtt = getticks() - ping_sent_ticks;
			if (ping_rtt < 1)
				ping_rtt = 1;
			ping_report();
			ping_phase = 2;
			return 1;
		}
		if (getticks() - ping_sent_ticks > PING_TIMEOUT) {
			if (ping_tries >= PING_TRIES) {
				ping_step = "timeout";
				return -1;
			}
			if (ping_send() != 0)
				return -1;
		}
	}
	return 0;
}

int
rump_ping_rx(struct mbuf *m)
{
	struct ip ip;
	struct icmp ic;
	uint8_t payload[PING_PAYLOAD];
	int hlen;

	if (m->m_pkthdr.len < (int)sizeof(ip))
		return -1;
	m_copydata(m, 0, sizeof(ip), &ip);
	if (ip.ip_v != IPVERSION || ip.ip_p != IPPROTO_ICMP)
		return -1;
	hlen = ip.ip_hl << 2;
	if (hlen < (int)sizeof(ip) ||
	    m->m_pkthdr.len < hlen + (int)sizeof(ic))
		return -1;
	m_copydata(m, hlen, sizeof(ic), &ic);
	if (ic.icmp_type != ICMP_ECHOREPLY ||
	    ntohs(ic.icmp_id) != PING_IDENT ||
	    ntohs(ic.icmp_seq) != (uint16_t)ping_last_seq)
		return -1;
	m_copydata(m, hlen + sizeof(ic), PING_PAYLOAD, payload);
	ping_reply_ok = memcmp(payload, ping_payload, PING_PAYLOAD) == 0;
	m_freem(m);
	return 0;
}
