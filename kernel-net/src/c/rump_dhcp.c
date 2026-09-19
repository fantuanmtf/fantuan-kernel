/* rump_dhcp.c - bounded DHCP client over the real UDP/socket layer (ours).
 * The e1000 has no address yet, and NetBSD's ip_output refuses a broadcast
 * from an interface with no in_ifaddr, so the client first gives the NIC a
 * provisional link-local 169.254.1.1/16 through in_control() and a host
 * route to the SLIRP server 10.0.2.2 through rtrequest1().  The exchange
 * itself is a real UDP socket bound to 0.0.0.0:68 and connected to
 * 10.0.2.2:67 (DISCOVER/OFFER/REQUEST/ACK, xid-checked, retries bounded).
 * Once the lease is known the address/mask go back through
 * in_control(SIOCAIFADDR), the provisional address is deleted and the
 * default gateway is installed in the real route table.  No blocking
 * waits: the net task polls.  The wire format is in rump_dhcp_pkt.c. */

#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include <sys/ioctl.h>
#include <sys/uio.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <sys/lwp.h>
#include <net/if.h>
#include <net/if_ether.h>
#include <net/route.h>
#include <netinet/in.h>
#include <netinet/in_systm.h>
#include <netinet/in_pcb.h>
#include <netinet/in_var.h>
#include "rump_shim.h"
#include "rump_dhcp.h"
#include "rump_e1000.h"

#define DHCP_PORT	68
#define DHCP_SERVER_PORT 67
#define DHCP_TRIES	5
#define DHCP_TIMEOUT	120	/* PIT ticks (100 Hz): 1.2 s per try */
#define DHCP_XID	0x46544e36u	/* "FTN6" */
#define LHOST_ADDR	0x0a000202u	/* 10.0.2.2 */

enum dhcp_phase {
	DHCP_IDLE, DHCP_PH_DISCOVER, DHCP_WAIT_OFFER, DHCP_WAIT_ACK,
	DHCP_DONE, DHCP_FAILED
};

static uint8_t dhcp_tx[576];
static struct socket *dhcp_so;
static struct dhcp_lease dhcp_lease, dhcp_offer;
static enum dhcp_phase dhcp_phase;
static uint8_t dhcp_mac[ETHER_ADDR_LEN];
static int dhcp_tries;
static uint64_t dhcp_sent;

static int
dhcp_fail(const char *step)
{

	printf("net: dhcp FAILED (%s)\n", step);
	dhcp_phase = DHCP_FAILED;
	return -1;
}

static int
dhcp_sock_open(void)
{
	struct sockaddr_in sin;
	int error;

	if (socreate(AF_INET, &dhcp_so, SOCK_DGRAM, 0, curlwp, NULL) != 0)
		return -1;
	dhcp_so->so_state |= SS_NBIO;
	dhcp_sin(&sin, INADDR_ANY, DHCP_PORT);
	if (sobind(dhcp_so, sintosa(&sin), curlwp) != 0)
		return -1;
	dhcp_sin(&sin, LHOST_ADDR, DHCP_SERVER_PORT);
	solock(dhcp_so);
	error = soconnect(dhcp_so, sintosa(&sin), curlwp);
	sounlock(dhcp_so);
	if (error != 0)
		return error;
	/* The server broadcasts its replies; a wildcard local address is
	 * what udp_input's broadcast delivery matches.  The route cached by
	 * soconnect still supplies the provisional source address. */
	in4p_laddr(sotoinpcb(dhcp_so)).s_addr = INADDR_ANY;
	return 0;
}

static int
dhcp_send(uint8_t type)
{
	struct iovec iov;
	struct uio uio;
	size_t len;
	int error;

	len = dhcp_pkt_build(type, DHCP_XID, dhcp_mac,
	    type == DHCP_MSG_REQUEST ? &dhcp_offer : NULL, dhcp_tx,
	    sizeof(dhcp_tx));
	if (len == 0)
		return -1;
	iov.iov_base = dhcp_tx;
	iov.iov_len = len;
	memset(&uio, 0, sizeof(uio));
	uio.uio_iov = &iov;
	uio.uio_iovcnt = 1;
	uio.uio_resid = len;
	uio.uio_rw = UIO_WRITE;
	error = sosend(dhcp_so, NULL, &uio, NULL, NULL, MSG_DONTWAIT, curlwp);
	if (error != 0 && error != EWOULDBLOCK)
		return -1;
	dhcp_sent = fantuan_rump_ticks();
	return 0;
}

static int
dhcp_recv(void)
{
	struct iovec iov;
	struct uio uio;
	uint8_t buf[1024];
	size_t n;
	int error, flags = MSG_DONTWAIT;

	if (dhcp_so->so_rcv.sb_cc == 0)
		return 0;
	iov.iov_base = buf;
	iov.iov_len = sizeof(buf);
	memset(&uio, 0, sizeof(uio));
	uio.uio_iov = &iov;
	uio.uio_iovcnt = 1;
	uio.uio_resid = sizeof(buf);
	uio.uio_rw = UIO_READ;
	error = soreceive(dhcp_so, NULL, &uio, NULL, NULL, &flags);
	if (error != 0)
		return -1;
	n = sizeof(buf) - uio.uio_resid;
	if (!dhcp_pkt_parse(buf, n, DHCP_XID, dhcp_mac, &dhcp_offer))
		return 0;
	if (dhcp_offer.server.s_addr == 0)
		dhcp_offer.server.s_addr = htonl(LHOST_ADDR);
	if (dhcp_offer.mask.s_addr == 0)
		dhcp_offer.mask.s_addr = htonl(0xffffff00u);
	if (dhcp_offer.gw.s_addr == 0)
		dhcp_offer.gw.s_addr = htonl(LHOST_ADDR);
	return 1;
}

void
rump_dhcp_begin(void)
{

	e1000_hw_mac(dhcp_mac);
	dhcp_phase = DHCP_PH_DISCOVER;
	dhcp_tries = 0;
}

int
rump_dhcp_poll(void)
{
	int r;

	if (dhcp_phase == DHCP_IDLE || dhcp_phase == DHCP_DONE)
		return 0;
	if (dhcp_phase == DHCP_FAILED)
		return -1;

	if (dhcp_phase == DHCP_PH_DISCOVER) {
		if (dhcp_if_provisional() != 0)
			return dhcp_fail("iface");
		if (dhcp_sock_open() != 0)
			return dhcp_fail("socket");
		if (dhcp_send(DHCP_MSG_DISCOVER) != 0)
			return dhcp_fail("discover");
		dhcp_tries = 1;
		dhcp_phase = DHCP_WAIT_OFFER;
		return 0;
	}

	r = dhcp_recv();
	if (r < 0)
		return dhcp_fail("recv");
	if (r > 0 && dhcp_phase == DHCP_WAIT_OFFER &&
	    dhcp_offer.type == DHCP_MSG_OFFER) {
		if (dhcp_send(DHCP_MSG_REQUEST) != 0)
			return dhcp_fail("request");
		dhcp_tries = 1;
		dhcp_phase = DHCP_WAIT_ACK;
		return 0;
	}
	if (r > 0 && dhcp_offer.type == DHCP_MSG_NAK)
		return dhcp_fail("nak");
	if (r > 0 && dhcp_phase == DHCP_WAIT_ACK &&
	    dhcp_offer.type == DHCP_MSG_ACK) {
		uint32_t m;
		int bits = 0;

		if (dhcp_if_apply(&dhcp_offer) != 0)
			return dhcp_fail("apply");
		dhcp_lease = dhcp_offer;
		m = ntohl(dhcp_lease.mask.s_addr);
		while (m != 0) {
			bits += (int)(m & 1);
			m >>= 1;
		}
		printf("net: dhcp lease %d.%d.%d.%d/%d gw %d.%d.%d.%d "
		    "dns %d.%d.%d.%d\n",
		    ((const uint8_t *)&dhcp_lease.addr.s_addr)[0],
		    ((const uint8_t *)&dhcp_lease.addr.s_addr)[1],
		    ((const uint8_t *)&dhcp_lease.addr.s_addr)[2],
		    ((const uint8_t *)&dhcp_lease.addr.s_addr)[3], bits,
		    ((const uint8_t *)&dhcp_lease.gw.s_addr)[0],
		    ((const uint8_t *)&dhcp_lease.gw.s_addr)[1],
		    ((const uint8_t *)&dhcp_lease.gw.s_addr)[2],
		    ((const uint8_t *)&dhcp_lease.gw.s_addr)[3],
		    ((const uint8_t *)&dhcp_lease.dns.s_addr)[0],
		    ((const uint8_t *)&dhcp_lease.dns.s_addr)[1],
		    ((const uint8_t *)&dhcp_lease.dns.s_addr)[2],
		    ((const uint8_t *)&dhcp_lease.dns.s_addr)[3]);
		dhcp_phase = DHCP_DONE;
		return 1;
	}

	if (fantuan_rump_ticks() - dhcp_sent > DHCP_TIMEOUT) {
		if (dhcp_tries >= DHCP_TRIES)
			return dhcp_fail(dhcp_phase == DHCP_WAIT_OFFER ?
			    "offer" : "ack");
		if (dhcp_send(dhcp_phase == DHCP_WAIT_OFFER ?
		    DHCP_MSG_DISCOVER : DHCP_MSG_REQUEST) != 0)
			return dhcp_fail("retry");
		dhcp_tries++;
	}
	return 0;
}

uint32_t
rump_dhcp_dns(void)
{

	return dhcp_lease.dns.s_addr;
}
