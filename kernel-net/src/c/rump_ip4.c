/* rump_ip4.c - real IPv4 stack bring-up and boot test state machine (ours).
 * Bring-up order: attach the inet/arp domains, run every protocol pr_init
 * (ip_init creates ip_pktq and the ipstat counters, icmp_init the wqinput
 * hook, udp_init the PCB table, arp_init the ARP packet queue), then the
 * real ifnet/route init and lo0, then the loopback and shim-ether IPv4
 * addresses through in_control(SIOCAIFADDR).  rump_net_poll() is called by
 * the loopback task: it drains every pktqueue (ip_input/arp input run here,
 * in task context) and walks the ping -> UDP -> ARP test phases, printing
 * the R4 boot markers.  Bounded: each step either completes or fails.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/mbuf.h>
#include <sys/domain.h>
#include <sys/protosw.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <sys/mutex.h>
#include <net/if.h>
#include <net/if_stats.h>
#include <netinet/in.h>
#include <netinet/in_var.h>
#include <netinet/ip_var.h>
#include "rump_shim.h"

extern struct domain inetdomain;
extern struct domain arpdomain;
extern int ip_dad_count;
extern struct ifnet *lo0ifp;
void lltableinit(void);

enum {
	IP4_PING,
	IP4_UDP,
	IP4_ARP_START,
	IP4_ARP_WAIT,
	IP4_DONE,
	IP4_STOP,
	IP4_FAILED
};

static int ip4_state;
static int ip4_bringup_ok;
static int ip4_udp_bytes;

static void
ip4_domain_attach(struct domain *dp)
{

	STAILQ_INSERT_TAIL(&domains, dp, dom_link);
}

static void
ip4_proto_init(const struct domain *dp)
{
	const struct protosw *pr;

	for (pr = dp->dom_protosw; pr < dp->dom_protoswNPROTOSW; pr++)
		if (pr->pr_init != NULL)
			(*pr->pr_init)();
}

static int
ip4_setaddr(struct ifnet *ifp, const char *name, uint32_t addr, uint32_t mask)
{
	struct in_aliasreq ifra;

	memset(&ifra, 0, sizeof(ifra));
	strlcpy(ifra.ifra_name, name, sizeof(ifra.ifra_name));
	ifra.ifra_addr.sin_len = sizeof(ifra.ifra_addr);
	ifra.ifra_addr.sin_family = AF_INET;
	ifra.ifra_addr.sin_addr.s_addr = addr;
	ifra.ifra_mask.sin_len = sizeof(ifra.ifra_mask);
	ifra.ifra_mask.sin_family = AF_INET;
	ifra.ifra_mask.sin_addr.s_addr = mask;
	return in_control(NULL, SIOCAIFADDR, &ifra, ifp);
}

static void
ip4_counters(unsigned long long *in, unsigned long long *out)
{
	struct if_data ifd;

	if_stats_to_if_data(rump_loopback_ifp(), &ifd, false);
	*in = (unsigned long long)ifd.ifi_ipackets;
	*out = (unsigned long long)ifd.ifi_opackets;
}

static int
ip4_fail(const char *step)
{

	printf("net: ip4 FAILED (%s)\n", step);
	ip4_state = IP4_FAILED;
	return 1;
}

void
rump_ip4_up(void)
{

	mutex_init(softnet_lock, MUTEX_DEFAULT, IPL_SOFTNET);
	ip_dad_count = 0;
	/* main() calls this upstream; it creates the llentry pool the ARP
	 * cache entries come from. */
	lltableinit();

	ip4_domain_attach(&inetdomain);
	ip4_domain_attach(&arpdomain);
	ip4_proto_init(&inetdomain);
	ip4_proto_init(&arpdomain);

	rump_loopback_up();
	if (!rump_loopback_ready()) {
		ip4_state = IP4_FAILED;
		return;
	}
	if (ip4_setaddr(lo0ifp, "lo0", htonl(INADDR_LOOPBACK),
	    htonl(IN_CLASSA_NET)) != 0) {
		ip4_state = IP4_FAILED;
		return;
	}
	if (rump_arp_up() != 0) {
		ip4_state = IP4_FAILED;
		return;
	}
	printf("net: lo0 up 127.0.0.1/8\n");
	rump_ping_begin();
	ip4_bringup_ok = 1;
}

int
rump_net_poll(void)
{
	unsigned long long in, out;
	int r;

	rump_pktq_drain();

	if (!ip4_bringup_ok)
		return ip4_fail("link");

	switch (ip4_state) {
	case IP4_PING:
		r = rump_ping_poll();
		if (r < 0)
			return ip4_fail("ping");
		if (r > 0)
			ip4_state = IP4_UDP;
		break;
	case IP4_UDP:
		ip4_udp_bytes = rump_udp_run();
		if (ip4_udp_bytes < 0)
			return ip4_fail("udp");
		printf("net: udp loopback ok (sent=1 recv=1 bytes=%d)\n",
		    ip4_udp_bytes);
		ip4_state = IP4_ARP_START;
		break;
	case IP4_ARP_START:
		rump_arp_test_start();
		ip4_state = IP4_ARP_WAIT;
		break;
	case IP4_ARP_WAIT:
		r = rump_arp_test_poll();
		if (r < 0)
			return ip4_fail("arp");
		if (r > 0) {
			printf("net: arp self-test ok (entries=%d)\n",
			    rump_arp_entries());
			ip4_state = IP4_DONE;
		}
		break;
	case IP4_DONE:
		ip4_counters(&in, &out);
		printf("net: in/out counters pkts_in=%llu pkts_out=%llu\n",
		    in, out);
		ip4_state = IP4_STOP;
		return 1;
	default:
		return 1;
	}
	return 0;
}
