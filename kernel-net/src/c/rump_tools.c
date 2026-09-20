/* rump_tools.c - M11 R7 boot self-test for the shell tools (ours).
 * Runs the same clients the `ping`/`nslookup`/`wget` shell commands use:
 * resolve test.fantuan against the offline host DNS server (explicit
 * override, since the DHCP resolver 10.0.2.3 cannot know the fixture
 * name), ICMP-echo the resolved address and HTTP-GET the fixture through
 * that name.  Bounded: each client carries its own retry/timeout budget;
 * the state machine just sequences them and prints the gate markers.
 * Interim bridge (C5): the catalog versions (apps/{ping,nslookup,wget})
 * take over at M14-4. */

#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include "rump_shim.h"

#define TOOLS_NAME	"test.fantuan"
#define TOOLS_DNS_SERVER 0x0a000202u	/* 10.0.2.2, SLIRP host alias */
#define TOOLS_DNS_PORT	5353		/* smoke-net DNS fixture */
#define TOOLS_HTTP_PORT	18080		/* smoke-net HTTP fixture */
#define TOOLS_LPORT	40001

enum {
	TOOLS_IDLE, TOOLS_DNS, TOOLS_PING, TOOLS_WGET, TOOLS_DONE,
	TOOLS_FAILED
};

static int tools_phase;
static uint32_t tools_addr;		/* host byte order */

void
rump_tools_begin(void)
{

	tools_addr = 0;
	tools_phase = TOOLS_DNS;
	rump_dns_start(TOOLS_NAME, strlen(TOOLS_NAME), TOOLS_DNS_SERVER,
	    TOOLS_DNS_PORT);
}

int
rump_tools_poll(void)
{
	int r;

	switch (tools_phase) {
	case TOOLS_IDLE:
		return 0;
	case TOOLS_DONE:
		return 1;
	case TOOLS_FAILED:
		return -1;
	case TOOLS_DNS:
		r = rump_dns_poll();
		if (r < 0) {
			printf("net: dns FAILED (%s)\n", rump_dns_error());
			tools_phase = TOOLS_FAILED;
			return -1;
		}
		if (r > 0) {
			tools_addr = rump_dns_result();
			printf("net: dns ok (name=%s addr=%d.%d.%d.%d)\n",
			    TOOLS_NAME,
			    (int)((tools_addr >> 24) & 0xff),
			    (int)((tools_addr >> 16) & 0xff),
			    (int)((tools_addr >> 8) & 0xff),
			    (int)(tools_addr & 0xff));
			rump_ping_begin_addr(tools_addr);
			tools_phase = TOOLS_PING;
		}
		break;
	case TOOLS_PING:
		r = rump_ping_poll();
		if (r < 0) {
			printf("net: tool FAILED (ping-%s)\n",
			    rump_ping_error());
			tools_phase = TOOLS_FAILED;
			return -1;
		}
		if (r > 0) {
			printf("net: ping %s ok (seq=%d rtt=%d ticks)\n",
			    TOOLS_NAME, rump_ping_seq(), rump_ping_rtt());
			rump_http_begin_wget(TOOLS_NAME, strlen(TOOLS_NAME),
			    tools_addr, TOOLS_HTTP_PORT, TOOLS_LPORT, 1);
			tools_phase = TOOLS_WGET;
		}
		break;
	case TOOLS_WGET:
		r = rump_http_poll();
		if (r < 0) {
			printf("net: tool FAILED (wget-%s)\n",
			    rump_http_error());
			tools_phase = TOOLS_FAILED;
			return -1;
		}
		if (r > 0)
			tools_phase = TOOLS_DONE;
		break;
	}
	return 0;
}
