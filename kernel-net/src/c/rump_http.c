/* rump_http.c - offline HTTP GET through the real TCP socket layer (ours).
 * After DHCP the e1000 has a lease and a default route, so this is an
 * ordinary non-blocking socket client: connect to HOST:PORT (the R6 gate
 * uses the SLIRP host alias 10.0.2.2:18080; R7's wget uses the address the
 * resolver returned), send a minimal HTTP/1.0 request, accumulate the
 * response and hash the entity body with FNV-1a.  The host-side server is
 * started by tools/smoke-net.sh.  All waits are PIT-tick bounded and driven
 * by the net task.  Response parsing lives in rump_http_pkt.c. */

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
#include "rump_http.h"

#define HTTP_PORT	18080
#define HTTP_HOST	0x0a000202u	/* 10.0.2.2 */
#define HTTP_LPORT	40000
#define HTTP_TIMEOUT	600	/* PIT ticks (100 Hz): 6 s */
#define HTTP_RX_MAX	8192
#define HTTP_HOST_MAX	64
#define HTTP_REQ_MAX	(128 + HTTP_HOST_MAX)

enum http_phase {
	HTTP_IDLE, HTTP_CONNECT, HTTP_SEND, HTTP_RECV, HTTP_DONE, HTTP_FAILED
};

static uint8_t http_rx[HTTP_RX_MAX];
static size_t http_rx_len;
static struct socket *http_so;
static enum http_phase http_phase;
static uint64_t http_t0;
static char http_host[HTTP_HOST_MAX];
static uint32_t http_addr;		/* host byte order */
static uint16_t http_port, http_lport;
static int http_marker;			/* print the boot marker */
static int http_wget;			/* marker tag: wget vs http get */
static int http_status;
static size_t http_bytes;
static uint32_t http_hash;
static const char *http_step;

const char *
rump_http_error(void)
{

	return http_step != NULL ? http_step : "unknown";
}

int
rump_http_status_code(void)
{

	return http_status;
}

size_t
rump_http_body_len(void)
{

	return http_bytes;
}

uint32_t
rump_http_body_hash(void)
{

	return http_hash;
}

static int
http_fail(const char *step)
{

	http_step = step;
	/* R7 wget failures are reported by the tools sequence with the
	 * `net: tool FAILED (...)` marker; the shell formats its own. */
	if (http_marker && !http_wget)
		printf("net: http FAILED (%s)\n", step);
	http_phase = HTTP_FAILED;
	return -1;
}

static int
http_connect(void)
{
	struct sockaddr_in sin;
	int error;

	if (socreate(AF_INET, &http_so, SOCK_STREAM, 0, curlwp, NULL) != 0)
		return -1;
	http_so->so_state |= SS_NBIO;
	memset(&sin, 0, sizeof(sin));
	sin.sin_len = sizeof(sin);
	sin.sin_family = AF_INET;
	sin.sin_addr.s_addr = htonl(INADDR_ANY);
	sin.sin_port = htons(http_lport);
	if (sobind(http_so, sintosa(&sin), curlwp) != 0)
		return -1;
	memset(&sin, 0, sizeof(sin));
	sin.sin_len = sizeof(sin);
	sin.sin_family = AF_INET;
	sin.sin_addr.s_addr = htonl(http_addr);
	sin.sin_port = htons(http_port);
	solock(http_so);
	error = soconnect(http_so, sintosa(&sin), curlwp);
	sounlock(http_so);
	return error;
}

static int
http_send(void)
{
	char req[HTTP_REQ_MAX];
	struct iovec iov;
	struct uio uio;
	int n;

	n = snprintf(req, sizeof(req),
	    "GET / HTTP/1.0\r\nHost: %s\r\nConnection: close\r\n\r\n",
	    http_host);
	if (n <= 0 || n >= (int)sizeof(req))
		return -1;
	iov.iov_base = req;
	iov.iov_len = (size_t)n;
	memset(&uio, 0, sizeof(uio));
	uio.uio_iov = &iov;
	uio.uio_iovcnt = 1;
	uio.uio_resid = iov.iov_len;
	uio.uio_rw = UIO_WRITE;
	if (sosend(http_so, NULL, &uio, NULL, NULL, MSG_DONTWAIT,
	    curlwp) != 0)
		return -1;
	http_t0 = fantuan_rump_ticks();
	return 0;
}

static int
http_recv(void)
{
	struct iovec iov;
	struct uio uio;
	int error, flags = MSG_DONTWAIT;

	if (http_rx_len >= sizeof(http_rx))
		return 1;
	iov.iov_base = http_rx + http_rx_len;
	iov.iov_len = sizeof(http_rx) - http_rx_len;
	memset(&uio, 0, sizeof(uio));
	uio.uio_iov = &iov;
	uio.uio_iovcnt = 1;
	uio.uio_resid = iov.iov_len;
	uio.uio_rw = UIO_READ;
	error = soreceive(http_so, NULL, &uio, NULL, NULL, &flags);
	if (error == EWOULDBLOCK)
		return 0;
	if (error != 0)
		return -1;
	/* uiomove() advances iov_len, so derive the total from the resid. */
	http_rx_len = sizeof(http_rx) - uio.uio_resid;
	return http_rx_len > 0 ? 1 : 0;
}

static int
http_complete(void)
{
	size_t off, have;
	int clen;

	if (rump_http_find_body(http_rx, http_rx_len, &off) == NULL)
		return 0;
	clen = rump_http_content_length(http_rx, off - 4);
	have = http_rx_len - off;
	if (clen >= 0)
		return (int)have >= clen;
	/* No Content-Length: require the connection to be closed. */
	return (http_so->so_state &
	    (SS_CANTRCVMORE | SS_ISDISCONNECTED)) != 0;
}

static int
http_report(void)
{
	size_t off;

	if (rump_http_find_body(http_rx, http_rx_len, &off) == NULL)
		return http_fail("response");
	http_bytes = http_rx_len - off;
	http_hash = rump_http_fnv1a(http_rx + off, http_bytes);
	http_status = rump_http_status(http_rx, off - 4);
	if (http_marker) {
		if (http_wget)
			printf("net: wget ok (url=http://%s:%u/ bytes=%lu "
			    "hash=%08x)\n", http_host, http_port,
			    (unsigned long)http_bytes, http_hash);
		else
			printf("net: http get ok (url=http://%s:%u/ "
			    "bytes=%lu hash=%08x)\n", http_host, http_port,
			    (unsigned long)http_bytes, http_hash);
	}
	soclose(http_so);
	http_so = NULL;
	http_phase = HTTP_DONE;
	return 1;
}

static void
http_configure(const char *host, size_t hostlen, uint32_t addr,
    uint16_t port, uint16_t lport, int marker, int wget)
{

	hostlen = hostlen < sizeof(http_host) - 1 ? hostlen :
	    sizeof(http_host) - 1;
	memcpy(http_host, host, hostlen);
	http_host[hostlen] = '\0';
	http_addr = addr;
	http_port = port;
	http_lport = lport;
	http_marker = marker;
	http_wget = wget;
	http_status = 0;
	http_bytes = 0;
	http_hash = 0;
	http_step = NULL;
	http_phase = HTTP_CONNECT;
	http_rx_len = 0;
}

void
rump_http_begin(void)
{

	http_configure("10.0.2.2", 8, HTTP_HOST, HTTP_PORT, HTTP_LPORT, 1, 0);
}

/* M11 R7 wget: HTTP GET from HOST (NUL-terminated by the caller) whose
 * address was resolved to ADDR (host byte order). */
void
rump_http_begin_wget(const char *host, size_t hostlen, uint32_t addr,
    uint16_t port, uint16_t lport, int marker)
{

	http_configure(host, hostlen, addr, port, lport, marker, 1);
}

int
rump_http_poll(void)
{
	int r;

	if (http_phase == HTTP_IDLE || http_phase == HTTP_DONE)
		return 0;
	if (http_phase == HTTP_FAILED)
		return -1;

	if (http_phase == HTTP_CONNECT) {
		r = http_connect();
		if (r != 0)
			return http_fail("connect");
		http_t0 = fantuan_rump_ticks();
		http_phase = HTTP_SEND;
		return 0;
	}
	if (http_phase == HTTP_SEND) {
		if (!rump_tcp_connected(http_so)) {
			if (fantuan_rump_ticks() - http_t0 > HTTP_TIMEOUT)
				return http_fail("connect");
			return 0;
		}
		if (http_send() != 0)
			return http_fail("send");
		http_phase = HTTP_RECV;
		return 0;
	}

	r = http_recv();
	if (r < 0)
		return http_fail("recv");
	if (r > 0 && http_complete())
		return http_report();
	if (fantuan_rump_ticks() - http_t0 > HTTP_TIMEOUT)
		return http_fail("recv");
	return 0;
}
