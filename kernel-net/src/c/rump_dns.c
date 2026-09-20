/* rump_dns.c - bounded DNS A-query client over the real UDP socket layer
 * (ours, M11 R7).  One query at a time: socreate/sosend/soreceive on a
 * connected non-blocking UDP socket, DNS_TRIES attempts spaced by
 * DNS_TIMEOUT PIT ticks.  The transaction id and the question echo are
 * verified by dns_pkt_parse(); no blocking waits, the net task polls. */

#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include <sys/uio.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <sys/lwp.h>
#include <net/if.h>
#include <netinet/in.h>
#include "rump_shim.h"
#include "rump_dhcp.h"
#include "rump_dns.h"

#define DNS_PORT	53
#define DNS_LPORT	41353
#define DNS_TRIES	3
#define DNS_TIMEOUT	100	/* PIT ticks (100 Hz): 1 s per try */
#define DNS_NAME_MAX	63

enum {
	DNS_IDLE, DNS_OPEN, DNS_WAIT, DNS_DONE, DNS_FAILED
};

static struct socket *dns_so;
static int dns_phase, dns_tries;
static uint16_t dns_id;
static uint64_t dns_sent;
static char dns_name[DNS_NAME_MAX + 1];
static size_t dns_namelen;
static uint32_t dns_server, dns_result;
static uint16_t dns_port;
static const char *dns_step;

/* DHCP-provided resolver, host byte order; 0 when there is no lease. */
uint32_t
rump_dns_default_server(void)
{

	return ntohl(rump_dhcp_dns());
}

const char *
rump_dns_error(void)
{

	return dns_step != NULL ? dns_step : "unknown";
}

static void
dns_close(void)
{

	if (dns_so != NULL) {
		soclose(dns_so);
		dns_so = NULL;
	}
}

static int
dns_fail(const char *step)
{

	dns_step = step;
	dns_phase = DNS_FAILED;
	dns_close();
	return -1;
}

static int
dns_sock_open(void)
{
	struct sockaddr_in sin;
	int error;

	if (socreate(AF_INET, &dns_so, SOCK_DGRAM, 0, curlwp, NULL) != 0)
		return -1;
	dns_so->so_state |= SS_NBIO;
	dhcp_sin(&sin, INADDR_ANY, DNS_LPORT);
	if (sobind(dns_so, sintosa(&sin), curlwp) != 0)
		return -1;
	dhcp_sin(&sin, dns_server, dns_port);
	solock(dns_so);
	error = soconnect(dns_so, sintosa(&sin), curlwp);
	sounlock(dns_so);
	return error;
}

static int
dns_send(void)
{
	uint8_t q[512];
	struct iovec iov;
	struct uio uio;
	size_t len;
	int error;

	len = dns_pkt_build(dns_name, dns_namelen, dns_id, q, sizeof(q));
	if (len == 0)
		return dns_fail("name");
	iov.iov_base = q;
	iov.iov_len = len;
	memset(&uio, 0, sizeof(uio));
	uio.uio_iov = &iov;
	uio.uio_iovcnt = 1;
	uio.uio_resid = len;
	uio.uio_rw = UIO_WRITE;
	error = sosend(dns_so, NULL, &uio, NULL, NULL, MSG_DONTWAIT, curlwp);
	if (error != 0 && error != EWOULDBLOCK)
		return dns_fail("send");
	dns_sent = fantuan_rump_ticks();
	return 0;
}

static int
dns_recv(void)
{
	uint8_t buf[512];
	struct iovec iov;
	struct uio uio;
	uint32_t addr;
	size_t n;
	int error, flags = MSG_DONTWAIT, r;

	if (dns_so->so_rcv.sb_cc == 0)
		return 0;
	iov.iov_base = buf;
	iov.iov_len = sizeof(buf);
	memset(&uio, 0, sizeof(uio));
	uio.uio_iov = &iov;
	uio.uio_iovcnt = 1;
	uio.uio_resid = sizeof(buf);
	uio.uio_rw = UIO_READ;
	error = soreceive(dns_so, NULL, &uio, NULL, NULL, &flags);
	if (error != 0)
		return dns_fail("recv");
	n = sizeof(buf) - uio.uio_resid;
	r = dns_pkt_parse(buf, n, dns_id, dns_name, dns_namelen, &addr);
	if (r == DNS_PKT_SHORT || r == DNS_PKT_ID || r == DNS_PKT_FORMAT)
		return 0;	/* not ours (or truncated): keep waiting */
	if (r == DNS_PKT_RCODE)
		return dns_fail("rcode");
	if (r == DNS_PKT_QUESTION)
		return dns_fail("question");
	if (r != DNS_PKT_OK)
		return dns_fail("answer");
	dns_result = addr;
	dns_phase = DNS_DONE;
	dns_close();
	return 1;
}

void
rump_dns_start(const char *name, size_t namelen, uint32_t server,
    uint16_t port)
{

	dns_namelen = namelen < DNS_NAME_MAX ? namelen : DNS_NAME_MAX;
	memcpy(dns_name, name, dns_namelen);
	dns_name[dns_namelen] = '\0';
	dns_server = server;
	dns_port = port;
	dns_id = (uint16_t)(fantuan_rump_ticks() ^
	    ((uint32_t)(uint8_t)name[0] << 8) ^ (uint32_t)namelen);
	dns_tries = 0;
	dns_result = 0;
	dns_step = NULL;
	dns_so = NULL;
	dns_phase = DNS_OPEN;
}

int
rump_dns_poll(void)
{
	int r;

	if (dns_phase == DNS_IDLE)
		return 0;
	if (dns_phase == DNS_DONE)
		return 1;
	if (dns_phase == DNS_FAILED)
		return -1;

	if (dns_phase == DNS_OPEN) {
		if (dns_sock_open() != 0)
			return dns_fail("socket");
		if (dns_send() != 0)
			return -1;
		dns_tries = 1;
		dns_phase = DNS_WAIT;
		return 0;
	}

	r = dns_recv();
	if (r != 0)
		return r;
	if (fantuan_rump_ticks() - dns_sent > DNS_TIMEOUT) {
		if (dns_tries >= DNS_TRIES)
			return dns_fail("timeout");
		if (dns_send() != 0)
			return -1;
		dns_tries++;
	}
	return 0;
}

uint32_t
rump_dns_result(void)
{

	return dns_result;
}
