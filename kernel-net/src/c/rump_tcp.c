/* rump_tcp.c - TCP-over-loopback bring-up test over the real socket layer
 * (ours).  The connection helpers live in rump_tcp_conn.c and the payload/
 * socket I/O in rump_tcp_io.c; this file is the bounded state machine that
 * prints the R5 markers.  Two connections are run: a clean one
 * (connect/transfer/hash/graceful close) and one with deterministic drops
 * armed through rump_loss_arm() to exercise retransmission.  Non-blocking
 * sockets and the poll below mean no blocking sleep path is used. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <netinet/in.h>
#include "rump_shim.h"

#define TCP_LOSS_DROPS	2
#define TCP_PORT_CA	25001
#define TCP_PORT_SA	25002
#define TCP_PORT_CB	25011
#define TCP_PORT_SB	25012
#define TCP_TIMEOUT	900	/* 9 s at the 10 ms net-task period */

enum tcp_state {
	TCP_SETUP_CLEAN,
	TCP_CONNECT_CLEAN,
	TCP_ACCEPT_CLEAN,
	TCP_SEND_CLEAN,
	TCP_RECV_CLEAN,
	TCP_SHUT_CLEAN,
	TCP_SETUP_LOSS,
	TCP_CONNECT_LOSS,
	TCP_ACCEPT_LOSS,
	TCP_SEND_LOSS,
	TCP_RECV_LOSS,
	TCP_SHUT_LOSS,
	TCP_DONE,
	TCP_FAILED
};

static struct socket *tcp_cl, *tcp_c, *tcp_s;
static struct lwp *tcp_lwp;
static size_t tcp_rx_len;
static int tcp_state, tcp_wait, tcp_shut_client, tcp_shut_server;
static uint64_t tcp_t0, tcp_ticks;

static int
tcp_fail(const char *step)
{

	printf("net: tcp FAILED (%s)\n", step);
	tcp_state = TCP_FAILED;
	return -1;
}

static int
tcp_setup(uint16_t cport, uint16_t sport)
{

	return rump_tcp_pair(cport, sport, &tcp_cl, &tcp_c, &tcp_s, tcp_lwp);
}

void
rump_tcp_begin(void)
{

	tcp_lwp = curlwp;
	rump_tcp_io_init();
	tcp_state = TCP_SETUP_CLEAN;
	tcp_wait = 0;
}

int
rump_tcp_poll(void)
{
	int r;

	rump_pktq_drain();
	tcp_wait++;
	if (tcp_wait > TCP_TIMEOUT &&
	    tcp_state != TCP_DONE && tcp_state != TCP_FAILED)
		return tcp_fail("timeout");

	switch (tcp_state) {
	case TCP_SETUP_CLEAN:
		if (tcp_setup(TCP_PORT_CA, TCP_PORT_SA) != 0)
			return tcp_fail("setup");
		tcp_wait = 0;
		tcp_state = TCP_CONNECT_CLEAN;
		break;
	case TCP_CONNECT_CLEAN:
		if (rump_tcp_connected(tcp_c)) {
			printf("net: tcp connect ok (state=ESTABLISHED)\n");
			tcp_state = TCP_ACCEPT_CLEAN;
		}
		break;
	case TCP_ACCEPT_CLEAN:
		r = rump_tcp_accept(tcp_cl, &tcp_s);
		if (r < 0)
			return tcp_fail("accept");
		if (r > 0) {
			tcp_t0 = fantuan_rump_ticks();
			tcp_state = TCP_SEND_CLEAN;
		}
		break;
	case TCP_SEND_CLEAN:
		r = rump_tcp_send(tcp_c, tcp_lwp);
		if (r < 0)
			return tcp_fail("send");
		if (r > 0)
			tcp_state = TCP_RECV_CLEAN;
		break;
	case TCP_RECV_CLEAN:
		r = rump_tcp_recv(tcp_s, &tcp_rx_len);
		if (r < 0)
			return tcp_fail("recv");
		if (r > 0) {
			if (rump_tcp_verify(tcp_rx_len) != 0)
				return tcp_fail("hash");
			tcp_ticks = fantuan_rump_ticks() - tcp_t0;
			printf("net: tcp transfer ok (bytes=%lu hash=%08x)\n",
			    (unsigned long)tcp_rx_len,
			    rump_tcp_rx_hash(tcp_rx_len));
			printf("net: tcp throughput ok (bytes=%lu ticks=%llu)\n",
			    (unsigned long)tcp_rx_len,
			    (unsigned long long)tcp_ticks);
			tcp_shut_client = tcp_shut_server = 0;
			tcp_state = TCP_SHUT_CLEAN;
		}
		break;
	case TCP_SHUT_CLEAN:
		rump_tcp_shutdown(tcp_c, tcp_s, &tcp_shut_client,
		    &tcp_shut_server);
		if ((tcp_c->so_state & SS_ISDISCONNECTED) != 0 &&
		    (tcp_s->so_state & SS_ISDISCONNECTED) != 0) {
			printf("net: tcp close ok (state=CLOSED)\n");
			rump_tcp_close(&tcp_cl, &tcp_c, &tcp_s);
			tcp_wait = 0;
			tcp_state = TCP_SETUP_LOSS;
		}
		break;
	case TCP_SETUP_LOSS:
		if (tcp_setup(TCP_PORT_CB, TCP_PORT_SB) != 0)
			return tcp_fail("loss-setup");
		rump_tcp_io_reset();
		tcp_rx_len = 0;
		tcp_wait = 0;
		tcp_state = TCP_CONNECT_LOSS;
		break;
	case TCP_CONNECT_LOSS:
		if (rump_tcp_connected(tcp_c))
			tcp_state = TCP_ACCEPT_LOSS;
		break;
	case TCP_ACCEPT_LOSS:
		r = rump_tcp_accept(tcp_cl, &tcp_s);
		if (r < 0)
			return tcp_fail("loss-accept");
		if (r > 0) {
			rump_loss_arm(TCP_PORT_CB, TCP_PORT_SB, TCP_LOSS_DROPS);
			tcp_state = TCP_SEND_LOSS;
		}
		break;
	case TCP_SEND_LOSS:
		r = rump_tcp_send(tcp_c, tcp_lwp);
		if (r < 0)
			return tcp_fail("loss-send");
		if (r > 0) {
			tcp_wait = 0;
			tcp_state = TCP_RECV_LOSS;
		}
		break;
	case TCP_RECV_LOSS:
		r = rump_tcp_recv(tcp_s, &tcp_rx_len);
		if (r < 0)
			return tcp_fail("loss-recv");
		if (r > 0 && rump_loss_retrans() >= rump_loss_dropped() &&
		    rump_loss_dropped() >= TCP_LOSS_DROPS) {
			if (rump_tcp_verify(tcp_rx_len) != 0)
				return tcp_fail("loss-hash");
			printf("net: tcp retransmit ok (drops=%u retrans=%u)\n",
			    rump_loss_dropped(), rump_loss_retrans());
			rump_loss_disarm();
			tcp_shut_client = tcp_shut_server = 0;
			tcp_state = TCP_SHUT_LOSS;
		}
		break;
	case TCP_SHUT_LOSS:
		rump_tcp_shutdown(tcp_c, tcp_s, &tcp_shut_client,
		    &tcp_shut_server);
		if ((tcp_c->so_state & SS_ISDISCONNECTED) != 0 &&
		    (tcp_s->so_state & SS_ISDISCONNECTED) != 0) {
			rump_tcp_close(&tcp_cl, &tcp_c, &tcp_s);
			tcp_state = TCP_DONE;
		}
		break;
	case TCP_DONE:
		return 1;
	case TCP_FAILED:
		return -1;
	default:
		return tcp_fail("state");
	}
	return 0;
}
