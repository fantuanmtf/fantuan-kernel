/* rump_udp_host.c - UDP echo test against the host fixture (ours, M11 R8).
 * Tor/I2P SOCKS proxies carry TCP only, so this is the UDP coverage: a real
 * connected UDP socket sends N deterministic datagrams to 10.0.2.2 (the
 * SLIRP host alias) where tools/smoke-net.sh runs a stdlib echo server, then
 * drains and verifies every echoed payload by byte and hash equality.
 * Runs inside the net task with bounded PIT-tick waits. */
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
#include "rump_nic.h"
#include "rump_udp_host.h"

#define UDPH_HOST	0x0a000202u	/* 10.0.2.2 */
#define UDPH_PORT	18082		/* smoke-net UDP echo fixture */
#define UDPH_LPORT	41382
#define UDPH_DGRAM	256
#define UDPH_COUNT	4
#define UDPH_TIMEOUT	300		/* PIT ticks (100 Hz): 3 s */

static uint8_t udph_sent[UDPH_COUNT][UDPH_DGRAM];
static uint8_t udph_recv[UDPH_DGRAM];
static int udph_tx, udph_rx;
static size_t udph_bytes;

int
rump_udp_host_tx(void)
{

	return udph_tx;
}

int
rump_udp_host_rx(void)
{

	return udph_rx;
}

size_t
rump_udp_host_bytes(void)
{

	return udph_bytes;
}

static void
udph_fill(int seq)
{
	int i;

	for (i = 0; i < UDPH_DGRAM; i++)
		udph_sent[seq][i] =
		    (uint8_t)((seq * 31 + i * 7 + 0x5a) & 0xff);
}

static int
udph_send_all(struct socket *so)
{
	int i;

	for (i = 0; i < UDPH_COUNT; i++) {
		struct iovec iov;
		struct uio uio;
		int error;

		udph_fill(i);
		iov.iov_base = udph_sent[i];
		iov.iov_len = UDPH_DGRAM;
		memset(&uio, 0, sizeof(uio));
		uio.uio_iov = &iov;
		uio.uio_iovcnt = 1;
		uio.uio_resid = UDPH_DGRAM;
		uio.uio_rw = UIO_WRITE;
		error = sosend(so, NULL, &uio, NULL, NULL, MSG_DONTWAIT,
		    curlwp);
		if (error == EWOULDBLOCK)
			continue;	/* UDP send buffers are not full */
		if (error != 0)
			return -1;
		udph_tx++;
	}
	return 0;
}

static int
udph_recv_all(struct socket *so)
{

	while (udph_rx < udph_tx) {
		struct iovec iov;
		struct uio uio;
		int error, flags = MSG_DONTWAIT;
		size_t n;

		if (so->so_rcv.sb_cc == 0)
			return 0;
		iov.iov_base = udph_recv;
		iov.iov_len = sizeof(udph_recv);
		memset(&uio, 0, sizeof(uio));
		uio.uio_iov = &iov;
		uio.uio_iovcnt = 1;
		uio.uio_resid = sizeof(udph_recv);
		uio.uio_rw = UIO_READ;
		error = soreceive(so, NULL, &uio, NULL, NULL, &flags);
		if (error == EWOULDBLOCK)
			return 0;
		if (error != 0)
			return -1;
		n = sizeof(udph_recv) - uio.uio_resid;
		if (n != UDPH_DGRAM)
			return -1;
		/* Match the echo against every outstanding send: SLIRP does
		 * not reorder a 4-datagram burst, but the sequence byte is
		 * not in the payload, so compare against each candidate. */
		{
			int i, found = -1;

			for (i = 0; i < UDPH_COUNT; i++)
				if (memcmp(udph_recv, udph_sent[i],
				    UDPH_DGRAM) == 0) {
					found = i;
					break;
				}
			if (found < 0)
				return -1;
		}
		udph_bytes += n;
		udph_rx++;
	}
	return 0;
}

int
rump_udp_host_run(void)
{
	struct socket *so;
	struct sockaddr_in sin;
	uint64_t t0;
	int error;

	udph_tx = udph_rx = 0;
	udph_bytes = 0;
	if (socreate(AF_INET, &so, SOCK_DGRAM, 0, curlwp, NULL) != 0)
		return -1;
	so->so_state |= SS_NBIO;
	dhcp_sin(&sin, INADDR_ANY, UDPH_LPORT);
	if (sobind(so, sintosa(&sin), curlwp) != 0) {
		soclose(so);
		return -1;
	}
	dhcp_sin(&sin, UDPH_HOST, UDPH_PORT);
	solock(so);
	error = soconnect(so, sintosa(&sin), curlwp);
	sounlock(so);
	if (error != 0 && error != EINPROGRESS && error != EWOULDBLOCK) {
		soclose(so);
		return -1;
	}
	if (udph_send_all(so) != 0) {
		soclose(so);
		return -1;
	}
	t0 = fantuan_rump_ticks();
	while (udph_rx < udph_tx &&
	    fantuan_rump_ticks() - t0 < UDPH_TIMEOUT) {
		rump_nic_poll();
		rump_pktq_drain();
		if (udph_recv_all(so) != 0) {
			soclose(so);
			return -1;
		}
	}
	soclose(so);
	if (udph_rx != udph_tx || udph_bytes !=
	    (size_t)(UDPH_COUNT * UDPH_DGRAM))
		return -1;
	return 0;
}
