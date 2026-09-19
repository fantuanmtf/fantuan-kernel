/* rump_udp.c - UDP loopback exchange over the real PCB path (ours).
 * Two fake sockets stand in for the R5 socket layer: each gets a real inpcb
 * created by inpcb_create(), bound to 127.0.0.1 with an explicit port and
 * (for the sender) connected to the peer.  The datagram is sent with the
 * real udp_output() and travels ip_output -> looutput -> ip_pktq; the net
 * task runs ip_input -> udp_input, which finds the receiver through
 * inpcb_lookup_bound() and queues the payload on its sockbuf via
 * rump_sock2.c's sbappendaddr().  The test then compares the payload and
 * checks the PCB states.  No blocking waits.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/mbuf.h>
#include <sys/errno.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <sys/protosw.h>
#include <sys/uidinfo.h>
#include <net/if.h>
#include <netinet/in.h>
#include <netinet/in_systm.h>
#include <netinet/in_pcb.h>
#include <netinet/in_var.h>
#include <netinet/ip.h>
#include <netinet/ip_var.h>
#include <netinet/udp.h>
#include <netinet/udp_var.h>
#include "rump_shim.h"

#define UDP_PORT_A	24001
#define UDP_PORT_B	24002
#define UDP_PAYLOAD	32

static struct socket udp_so_a;
static struct socket udp_so_b;
static struct uidinfo udp_uidinfo;

static uint32_t
udp_hash(const uint8_t *p, size_t n)
{
	uint32_t h = 2166136261u;

	while (n-- > 0) {
		h ^= *p++;
		h *= 16777619u;
	}
	return h;
}

static int
udp_endpoint_init(struct socket *so)
{
	const struct protosw *pr;
	int error;

	pr = pffindproto(AF_INET, IPPROTO_UDP, SOCK_DGRAM);
	if (pr == NULL)
		return -1;
	so->so_type = SOCK_DGRAM;
	so->so_proto = pr;
	so->so_uidinfo = &udp_uidinfo;
	/* R5's socreate() reserves these; the fake endpoints need the
	 * sockbuf watermarks, or sbappendaddr() drops every datagram. */
	so->so_rcv.sb_so = so;
	so->so_rcv.sb_hiwat = 4096;
	so->so_rcv.sb_mbmax = 65536;
	so->so_rcv.sb_lowat = 1;
	so->so_snd = so->so_rcv;
	error = inpcb_create(so, &udbtable);
	if (error != 0)
		return error;
	/* udp_attach() normally seeds the protocol control block defaults. */
	in4p_ip(sotoinpcb(so)).ip_ttl = ip_defttl;
	in4p_ip(sotoinpcb(so)).ip_tos = 0;
	return 0;
}

static void
udp_sin_init(struct sockaddr_in *sin, uint16_t port)
{

	memset(sin, 0, sizeof(*sin));
	sin->sin_len = sizeof(*sin);
	sin->sin_family = AF_INET;
	sin->sin_port = htons(port);
	sin->sin_addr.s_addr = htonl(INADDR_LOOPBACK);
}

static int
udp_pcb_checks(struct inpcb *inp_a, struct inpcb *inp_b)
{
	struct sockaddr_in sin_a, sin_b, sin;
	const struct inpcb *found;

	udp_sin_init(&sin_a, UDP_PORT_A);
	udp_sin_init(&sin_b, UDP_PORT_B);

	found = inpcb_lookup(&udbtable, sin_b.sin_addr, sin_b.sin_port,
	    sin_a.sin_addr, sin_a.sin_port, NULL);
	if (found != inp_a)
		return -1;

	found = inpcb_lookup_bound(&udbtable, sin_b.sin_addr, sin_b.sin_port);
	if (found != inp_b)
		return -1;

	udp_sin_init(&sin, UDP_PORT_B + 1);
	if (inpcb_lookup_bound(&udbtable, sin.sin_addr, sin.sin_port) != NULL)
		return -1;

	if (inp_a->inp_state != INP_CONNECTED ||
	    inp_b->inp_state != INP_BOUND)
		return -1;

	inpcb_fetch_sockaddr(inp_b, &sin);
	if (sin.sin_port != htons(UDP_PORT_B) ||
	    sin.sin_addr.s_addr != htonl(INADDR_LOOPBACK))
		return -1;
	return 0;
}

static int
udp_recv_copy(uint8_t *buf, size_t max)
{
	struct mbuf *m;
	size_t n = 0;

	for (m = udp_so_b.so_rcv.sb_mb; m != NULL; m = m->m_next) {
		if (m->m_type == MT_SONAME)
			continue;
		if (n + (size_t)m->m_len > max)
			return -1;
		memcpy(buf + n, mtod(m, void *), m->m_len);
		n += (size_t)m->m_len;
	}
	return (int)n;
}

int
rump_udp_run(void)
{
	struct sockaddr_in sin_a, sin_b;
	struct inpcb *inp_a, *inp_b;
	struct mbuf *m;
	uint8_t sendbuf[UDP_PAYLOAD], recvbuf[UDP_PAYLOAD];
	int i, n, error;

	if (udp_endpoint_init(&udp_so_a) != 0 ||
	    udp_endpoint_init(&udp_so_b) != 0)
		return -1;
	inp_a = sotoinpcb(&udp_so_a);
	inp_b = sotoinpcb(&udp_so_b);

	udp_sin_init(&sin_a, UDP_PORT_A);
	udp_sin_init(&sin_b, UDP_PORT_B);
	if (inpcb_bind(inp_a, &sin_a, &lwp0) != 0 ||
	    inpcb_bind(inp_b, &sin_b, &lwp0) != 0)
		return -1;
	if (inpcb_connect(inp_a, &sin_b, &lwp0) != 0)
		return -1;
	if (udp_pcb_checks(inp_a, inp_b) != 0)
		return -1;

	for (i = 0; i < UDP_PAYLOAD; i++)
		sendbuf[i] = (uint8_t)("fantuan-r4-udp"[i % 14] + i);

	m = m_gethdr(M_DONTWAIT, MT_DATA);
	if (m == NULL)
		return -1;
	memcpy(mtod(m, void *), sendbuf, UDP_PAYLOAD);
	m->m_len = m->m_pkthdr.len = UDP_PAYLOAD;
	error = udp_output(m, inp_a, NULL, &lwp0);
	if (error != 0)
		return -1;

	rump_pktq_drain();

	n = udp_recv_copy(recvbuf, sizeof(recvbuf));
	if (n != UDP_PAYLOAD)
		return -1;
	if (memcmp(recvbuf, sendbuf, UDP_PAYLOAD) != 0)
		return -1;
	if (udp_hash(recvbuf, UDP_PAYLOAD) != udp_hash(sendbuf, UDP_PAYLOAD))
		return -1;
	return n;
}
