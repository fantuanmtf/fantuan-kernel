/* rump_tls_io.c - socket BIO, request build and response for the TLS client
 * (ours, M11 R8).  Split from rump_tls.c to keep every adapter file inside
 * the size rule; the session driver is rump_tls.c, config in
 * rump_tls_conf.c.  Response parsing reuses rump_http_pkt.c. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include <sys/uio.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <sys/lwp.h>
#include "rump_shim.h"
#include "rump_http.h"
#include "rump_tls.h"
#include "rump_tls_int.h"

#include <stdarg.h>
#include <stdio.h>
#include <string.h>

void rump_tls_capture(const uint8_t *hdr, size_t len);

static int
tls_bio_send(void *ctx, const unsigned char *buf, size_t len)
{
	struct iovec iov;
	struct uio uio;
	int error;

	(void)ctx;
	iov.iov_base = (void *)(uintptr_t)buf;
	iov.iov_len = len;
	memset(&uio, 0, sizeof(uio));
	uio.uio_iov = &iov;
	uio.uio_iovcnt = 1;
	uio.uio_resid = len;
	uio.uio_rw = UIO_WRITE;
	error = sosend(tls_so, NULL, &uio, NULL, NULL, MSG_DONTWAIT, curlwp);
	if (error == EWOULDBLOCK)
		return MBEDTLS_ERR_SSL_WANT_WRITE;
	if (error != 0)
		return -1;
	return (int)(len - uio.uio_resid);
}

static int
tls_bio_recv(void *ctx, unsigned char *buf, size_t len)
{
	struct iovec iov;
	struct uio uio;
	int error, flags = MSG_DONTWAIT;
	size_t n;

	(void)ctx;
	if (tls_so->so_rcv.sb_cc == 0)
		return MBEDTLS_ERR_SSL_WANT_READ;
	iov.iov_base = buf;
	iov.iov_len = len;
	memset(&uio, 0, sizeof(uio));
	uio.uio_iov = &iov;
	uio.uio_iovcnt = 1;
	uio.uio_resid = len;
	uio.uio_rw = UIO_READ;
	error = soreceive(tls_so, NULL, &uio, NULL, NULL, &flags);
	if (error == EWOULDBLOCK)
		return MBEDTLS_ERR_SSL_WANT_READ;
	if (error != 0)
		return -1;
	n = len - uio.uio_resid;
	return n > 0 ? (int)n : MBEDTLS_ERR_SSL_WANT_READ;
}

int
tls_handshake_start(void)
{
	int r;

	mbedtls_ssl_init(&tls_ssl);
	r = mbedtls_ssl_setup(&tls_ssl, &tls_conf);
	if (r != 0)
		return tls_fail("setup");
	tls_ssl_up = 1;
	if (tls_verify && tls_ca_ok)
		mbedtls_ssl_conf_ca_chain(&tls_conf, &tls_ca, NULL);
	mbedtls_ssl_conf_authmode(&tls_conf, tls_verify && tls_ca_ok ?
	    MBEDTLS_SSL_VERIFY_REQUIRED : MBEDTLS_SSL_VERIFY_NONE);
	if (mbedtls_ssl_set_hostname(&tls_ssl, tls_host) != 0)
		return tls_fail("hostname");
	mbedtls_ssl_set_bio(&tls_ssl, tls_so, tls_bio_send, tls_bio_recv, NULL);
	tls_phase = TLS_HANDSHAKE;
	return 0;
}

static unsigned
tls_append(char *dst, size_t cap, size_t off, const char *fmt, ...)
{
	va_list ap;
	int n;

	if (off >= cap)
		return 0;
	va_start(ap, fmt);
	n = vsnprintf(dst + off, cap - off, fmt, ap);
	va_end(ap);
	if (n <= 0 || (size_t)n >= cap - off)
		return 0;
	return (unsigned)n;
}

int
tls_build_request(void)
{
	size_t off = 0;
	unsigned n;

	n = tls_append((char *)tls_tx, sizeof(tls_tx), off,
	    "%s %s HTTP/1.0\r\nHost: %s\r\nUser-Agent: fantuan-wget/0.0.3\r\n"
	    "Accept: */*\r\nConnection: close\r\n",
	    tls_method, tls_path, tls_host);
	if (n == 0)
		return tls_fail("request");
	off += n;
	if (tls_ctype[0] != '\0') {
		n = tls_append((char *)tls_tx, sizeof(tls_tx), off,
		    "Content-Type: %s\r\n", tls_ctype);
		if (n == 0)
			return tls_fail("request");
		off += n;
	}
	if (tls_bodylen > 0) {
		n = tls_append((char *)tls_tx, sizeof(tls_tx), off,
		    "Content-Length: %lu\r\n", (unsigned long)tls_bodylen);
		if (n == 0)
			return tls_fail("request");
		off += n;
	}
	if (tls_cookie[0] != '\0') {
		n = tls_append((char *)tls_tx, sizeof(tls_tx), off,
		    "Cookie: %s\r\n", tls_cookie);
		if (n == 0)
			return tls_fail("request");
		off += n;
	}
	if (tls_vqd[0] != '\0') {
		n = tls_append((char *)tls_tx, sizeof(tls_tx), off,
		    "x-vqd-4: %s\r\n", tls_vqd);
		if (n == 0)
			return tls_fail("request");
		off += n;
	}
	n = tls_append((char *)tls_tx, sizeof(tls_tx), off, "\r\n%s",
	    tls_body);
	if (n == 0)
		return tls_fail("request");
	tls_tx_len = off + n;
	tls_tx_off = 0;
	return 0;
}

int
tls_recv_step(void)
{
	unsigned char tmp[4096];
	int r;
	size_t space;

	r = mbedtls_ssl_read(&tls_ssl, tmp, sizeof(tmp));
	if (r == MBEDTLS_ERR_SSL_WANT_READ || r == MBEDTLS_ERR_SSL_WANT_WRITE)
		return 0;
	if (r == MBEDTLS_ERR_SSL_PEER_CLOSE_NOTIFY || r == 0) {
		if (rump_http_find_body(tls_rx, tls_rx_len, &tls_body_off) !=
		    NULL)
			return 1;
		return tls_fail("recv");
	}
	if (r < 0)
		return tls_fail("recv");
	space = sizeof(tls_rx) - tls_rx_len;
	if ((size_t)r > space) {
		tls_truncated = 1;
		r = (int)space;
	}
	memcpy(tls_rx + tls_rx_len, tmp, (size_t)r);
	tls_rx_len += (size_t)r;
	if (rump_http_find_body(tls_rx, tls_rx_len, &tls_body_off) != NULL) {
		int clen = rump_http_content_length(tls_rx,
		    tls_body_off - 4);
		if (tls_truncated)
			return 1;
		if (clen >= 0 && tls_rx_len - tls_body_off >= (size_t)clen)
			return 1;
	}
	return 0;
}

int
tls_report(void)
{
	const uint8_t *body;

	body = rump_http_find_body(tls_rx, tls_rx_len, &tls_body_off);
	if (body == NULL)
		return tls_fail("response");
	tls_bytes = tls_rx_len - tls_body_off;
	tls_hash = rump_http_fnv1a(body, tls_bytes);
	tls_status = rump_http_status(tls_rx, tls_body_off - 4);
	if (tls_capture)
		rump_tls_capture(tls_rx, tls_body_off - 4);
	tls_phase = TLS_DONE;
	tls_close();
	return 1;
}
