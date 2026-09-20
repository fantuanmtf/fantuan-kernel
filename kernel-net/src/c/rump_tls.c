/* rump_tls.c - TLS session driver over the rump socket layer (ours).
 * One session at a time, polled by the net task: TCP connect, mbedTLS
 * handshake (TLS 1.2), one HTTP request, response and cleanup.  The BIO is
 * the real socket; all waits are PIT-tick bounded.  Verification uses the
 * pinned CA embedded by the build; the optional external phase runs the
 * same path with verification off.  The config/init and request spec live
 * in rump_tls_conf.c, the transport/HTTP pieces in rump_tls_io.c. */
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
#include "rump_tls.h"
#include "rump_tls_int.h"

#include <string.h>

struct socket *tls_so;
mbedtls_ssl_context tls_ssl;
int tls_ssl_up, tls_phase, tls_verify;
uint64_t tls_t0;
uint32_t tls_timeout;
const char *tls_step;

char tls_host[TLS_HOST_MAX];
uint8_t tls_tx[TLS_TX_MAX], tls_rx[TLS_RX_MAX];
size_t tls_tx_len, tls_tx_off, tls_rx_len, tls_body_off;
int tls_status, tls_truncated;
size_t tls_bytes;
uint32_t tls_hash;

const char *
rump_tls_error(void)
{

	return tls_step != NULL ? tls_step : "unknown";
}

int
rump_tls_status_code(void)
{

	return tls_status;
}

size_t
rump_tls_body_len(void)
{

	return tls_bytes;
}

uint32_t
rump_tls_body_hash(void)
{

	return tls_hash;
}

const uint8_t *
rump_tls_body(size_t *len)
{

	if (len != NULL)
		*len = tls_bytes;
	return tls_rx + tls_body_off;
}

int
rump_tls_truncated(void)
{

	return tls_truncated;
}

const char *
rump_tls_cookie(void)
{

	return tls_cookie;
}

const char *
rump_tls_vqd(void)
{

	return tls_vqd;
}

void
tls_close(void)
{

	if (tls_ssl_up) {
		mbedtls_ssl_free(&tls_ssl);
		tls_ssl_up = 0;
	}
	if (tls_so != NULL) {
		soclose(tls_so);
		tls_so = NULL;
	}
}

int
tls_fail(const char *step)
{

	tls_step = step;
	tls_phase = TLS_FAILED;
	tls_close();
	return -1;
}

void
rump_tls_begin(const char *host, size_t hostlen, uint32_t addr, uint16_t port,
    uint16_t lport, int verify, uint32_t timeout)
{
	struct sockaddr_in sin;
	int error;

	if (!tls_inited && rump_tls_init() != 0) {
		tls_fail("init");
		return;
	}
	hostlen = hostlen < sizeof(tls_host) - 1 ? hostlen :
	    sizeof(tls_host) - 1;
	memcpy(tls_host, host, hostlen);
	tls_host[hostlen] = '\0';
	tls_verify = verify;
	tls_timeout = timeout;
	tls_step = NULL;
	tls_rx_len = tls_body_off = tls_bytes = tls_hash = 0;
	tls_status = tls_truncated = 0;
	tls_so = NULL;
	if (socreate(AF_INET, &tls_so, SOCK_STREAM, 0, curlwp, NULL) != 0) {
		tls_fail("socket");
		return;
	}
	tls_so->so_state |= SS_NBIO;
	memset(&sin, 0, sizeof(sin));
	sin.sin_len = sizeof(sin);
	sin.sin_family = AF_INET;
	sin.sin_addr.s_addr = htonl(INADDR_ANY);
	sin.sin_port = htons(lport);
	if (sobind(tls_so, sintosa(&sin), curlwp) != 0) {
		tls_fail("bind");
		return;
	}
	memset(&sin, 0, sizeof(sin));
	sin.sin_len = sizeof(sin);
	sin.sin_family = AF_INET;
	sin.sin_addr.s_addr = htonl(addr);
	sin.sin_port = htons(port);
	solock(tls_so);
	error = soconnect(tls_so, sintosa(&sin), curlwp);
	sounlock(tls_so);
	tls_t0 = fantuan_rump_ticks();
	tls_phase = TLS_CONNECT;
	if (error != 0 && error != EINPROGRESS && error != EWOULDBLOCK) {
		tls_fail("connect");
		return;
	}
}

int
rump_tls_poll(void)
{
	int r;

	if (tls_phase == TLS_IDLE)
		return 0;
	if (tls_phase == TLS_DONE)
		return 1;
	if (tls_phase == TLS_FAILED)
		return -1;
	if (fantuan_rump_ticks() - tls_t0 > tls_timeout)
		return tls_fail(tls_phase == TLS_CONNECT ? "connect" :
		    tls_phase == TLS_HANDSHAKE ? "handshake" :
		    tls_phase == TLS_SEND ? "send" : "recv");

	if (tls_phase == TLS_CONNECT) {
		if ((tls_so->so_state & SS_ISDISCONNECTED) != 0)
			return tls_fail("connect");
		if (!rump_tcp_connected(tls_so))
			return 0;
		return tls_handshake_start();
	}
	if (tls_phase == TLS_HANDSHAKE) {
		r = mbedtls_ssl_handshake(&tls_ssl);
		if (r == MBEDTLS_ERR_SSL_WANT_READ ||
		    r == MBEDTLS_ERR_SSL_WANT_WRITE)
			return 0;
		if (r != 0)
			return tls_fail("handshake");
		if (tls_verify && (mbedtls_ssl_get_verify_result(&tls_ssl) != 0))
			return tls_fail("verify");
		if (tls_build_request() != 0)
			return -1;
		tls_phase = TLS_SEND;
		return 0;
	}
	if (tls_phase == TLS_SEND) {
		r = mbedtls_ssl_write(&tls_ssl, tls_tx + tls_tx_off,
		    tls_tx_len - tls_tx_off);
		if (r == MBEDTLS_ERR_SSL_WANT_READ ||
		    r == MBEDTLS_ERR_SSL_WANT_WRITE)
			return 0;
		if (r < 0)
			return tls_fail("send");
		tls_tx_off += (size_t)r;
		if (tls_tx_off >= tls_tx_len) {
			tls_rx_len = 0;
			tls_phase = TLS_RECV;
		}
		return 0;
	}

	r = tls_recv_step();
	if (r < 0)
		return r;
	if (r > 0)
		return tls_report();
	return 0;
}
