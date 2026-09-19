/* rump_tcp_io.c - payload, hash and non-blocking socket I/O for the R5 TCP
 * test (ours).  The state machine in rump_tcp.c owns the connection phases;
 * this file owns the deterministic 64 KiB blob and the sosend()/soreceive()
 * steps, all with MSG_DONTWAIT so the cooperative net task never blocks. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include <sys/uio.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <sys/mbuf.h>
#include <netinet/in.h>
#include "rump_shim.h"

#define TCP_BLOB	65536

static uint8_t tcp_tx[TCP_BLOB];
static uint8_t tcp_rx[TCP_BLOB];
static size_t tcp_tx_off, tcp_rx_len;

void
rump_tcp_io_reset(void)
{

	tcp_tx_off = 0;
	tcp_rx_len = 0;
}

void
rump_tcp_io_init(void)
{
	size_t i;

	for (i = 0; i < sizeof(tcp_tx); i++)
		tcp_tx[i] = (uint8_t)("fantuan-r5-tcp"[i % 14] ^ (i * 7));
	rump_tcp_io_reset();
}

uint32_t
rump_tcp_hash(const uint8_t *p, size_t n)
{
	uint32_t h = 2166136261u;

	while (n-- > 0) {
		h ^= *p++;
		h *= 16777619u;
	}
	return h;
}

uint32_t
rump_tcp_rx_hash(size_t len)
{

	return rump_tcp_hash(tcp_rx, len);
}

int
rump_tcp_verify(size_t len)
{

	if (len != sizeof(tcp_tx))
		return -1;
	return memcmp(tcp_rx, tcp_tx, sizeof(tcp_tx)) == 0 ? 0 : -1;
}

int
rump_tcp_send(struct socket *so, struct lwp *l)
{
	struct iovec iov;
	struct uio uio;
	int error;

	iov.iov_base = tcp_tx + tcp_tx_off;
	iov.iov_len = sizeof(tcp_tx) - tcp_tx_off;
	memset(&uio, 0, sizeof(uio));
	uio.uio_iov = &iov;
	uio.uio_iovcnt = 1;
	uio.uio_resid = iov.iov_len;
	uio.uio_rw = UIO_WRITE;
	error = sosend(so, NULL, &uio, NULL, NULL, MSG_DONTWAIT, l);
	tcp_tx_off = sizeof(tcp_tx) - uio.uio_resid;
	if (error == 0)
		return tcp_tx_off == sizeof(tcp_tx) ? 1 : 0;
	if (error == EWOULDBLOCK)
		return 0;
	return -1;
}

int
rump_tcp_recv(struct socket *so, size_t *len)
{
	struct iovec iov;
	struct uio uio;
	int error, flags = MSG_DONTWAIT;

	for (;;) {
		size_t before;

		if (tcp_rx_len >= sizeof(tcp_rx)) {
			*len = tcp_rx_len;
			return 1;
		}
		if (so->so_rcv.sb_cc == 0) {
			*len = tcp_rx_len;
			return 0;
		}
		iov.iov_base = tcp_rx + tcp_rx_len;
		iov.iov_len = sizeof(tcp_rx) - tcp_rx_len;
		memset(&uio, 0, sizeof(uio));
		uio.uio_iov = &iov;
		uio.uio_iovcnt = 1;
		uio.uio_resid = iov.iov_len;
		uio.uio_rw = UIO_READ;
		before = uio.uio_resid;
		error = soreceive(so, NULL, &uio, NULL, NULL, &flags);
		tcp_rx_len = sizeof(tcp_rx) - uio.uio_resid;
		*len = tcp_rx_len;
		if (error == EWOULDBLOCK)
			return 0;
		if (error != 0)
			return -1;
		if (uio.uio_resid == before)
			return -1;	/* no progress; never spin here */
	}
}
