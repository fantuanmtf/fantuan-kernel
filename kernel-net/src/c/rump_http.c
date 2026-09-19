/* rump_http.c - offline HTTP GET through the real TCP socket layer (ours).
 * After DHCP the e1000 has a lease and a default route, so this is an
 * ordinary non-blocking socket client: connect to the SLIRP host alias
 * 10.0.2.2:HTTP_PORT, send a minimal HTTP/1.0 request, accumulate the
 * response and hash the entity body with FNV-1a.  The host-side server is
 * started by tools/smoke-net.sh.  All waits are PIT-tick bounded and driven
 * by the net task. */

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

#define HTTP_PORT	18080
#define HTTP_HOST	0x0a000202u	/* 10.0.2.2 */
#define HTTP_LPORT	40000
#define HTTP_TIMEOUT	600	/* PIT ticks (100 Hz): 6 s */
#define HTTP_RX_MAX	8192

enum http_phase {
	HTTP_IDLE, HTTP_CONNECT, HTTP_SEND, HTTP_RECV, HTTP_DONE, HTTP_FAILED
};

static const char http_req[] =
    "GET / HTTP/1.0\r\nHost: 10.0.2.2\r\nConnection: close\r\n\r\n";

static uint8_t http_rx[HTTP_RX_MAX];
static size_t http_rx_len;
static struct socket *http_so;
static enum http_phase http_phase;
static uint64_t http_t0;

static uint32_t
http_hash(const uint8_t *p, size_t n)
{
	uint32_t h = 2166136261u;

	while (n-- > 0) {
		h ^= *p++;
		h *= 16777619u;
	}
	return h;
}

static int
http_fail(const char *step)
{

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
	sin.sin_port = htons(HTTP_LPORT);
	if (sobind(http_so, sintosa(&sin), curlwp) != 0)
		return -1;
	memset(&sin, 0, sizeof(sin));
	sin.sin_len = sizeof(sin);
	sin.sin_family = AF_INET;
	sin.sin_addr.s_addr = htonl(HTTP_HOST);
	sin.sin_port = htons(HTTP_PORT);
	solock(http_so);
	error = soconnect(http_so, sintosa(&sin), curlwp);
	sounlock(http_so);
	return error;
}

static int
http_send(void)
{
	struct iovec iov;
	struct uio uio;

	iov.iov_base = (void *)(uintptr_t)http_req;
	iov.iov_len = sizeof(http_req) - 1;
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

static const uint8_t *
http_find_body(size_t *off)
{
	static const char sep[] = "\r\n\r\n";
	size_t i;

	for (i = 0; i + sizeof(sep) - 1 <= http_rx_len; i++)
		if (memcmp(http_rx + i, sep, sizeof(sep) - 1) == 0) {
			*off = i + sizeof(sep) - 1;
			return http_rx + *off;
		}
	return NULL;
}

static int
http_content_length(size_t hlen)
{
	static const char key[] = "content-length:";
	size_t i, j;

	for (i = 0; i + sizeof(key) - 1 <= hlen; i++) {
		int match = 1;

		for (j = 0; j < sizeof(key) - 1; j++) {
			char c = (char)http_rx[i + j];

			if (c >= 'A' && c <= 'Z')
				c = (char)(c - 'A' + 'a');
			if (c != key[j]) {
				match = 0;
				break;
			}
		}
		if (!match)
			continue;
		i += sizeof(key) - 1;
		while (i < hlen && (http_rx[i] == ' ' ||
		    http_rx[i] == '\t'))
			i++;
		{
			int v = 0;

			while (i < hlen && http_rx[i] >= '0' &&
			    http_rx[i] <= '9')
				v = v * 10 + (http_rx[i++] - '0');
			return v;
		}
	}
	return -1;
}

static int
http_complete(void)
{
	const uint8_t *body;
	size_t off, have;
	int clen;

	body = http_find_body(&off);
	if (body == NULL)
		return 0;
	clen = http_content_length(off - 4);
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
	const uint8_t *body;
	size_t off, len;
	uint32_t hash;

	body = http_find_body(&off);
	if (body == NULL)
		return http_fail("response");
	len = http_rx_len - off;
	hash = http_hash(body, len);
	printf("net: http get ok (url=http://10.0.2.2:%d/ bytes=%lu "
	    "hash=%08x)\n", HTTP_PORT, (unsigned long)len, hash);
	soclose(http_so);
	http_so = NULL;
	http_phase = HTTP_DONE;
	return 1;
}

void
rump_http_begin(void)
{

	http_phase = HTTP_CONNECT;
	http_rx_len = 0;
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
