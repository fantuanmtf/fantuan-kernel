/* rump_tcp_conn.c - connection helpers for the R5 TCP loopback test (ours).
 * Creates the real listening/client sockets and runs the non-blocking
 * socket calls (bind/listen/connect), pops the completed connection from the
 * listener's accept queue, shuts both directions down and closes the trio.
 * The state machine in rump_tcp.c sequences these calls. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <sys/mbuf.h>
#include <net/if.h>
#include <netinet/in.h>
#include <netinet/in_systm.h>
#include <netinet/ip.h>
#include <netinet/in_pcb.h>
#include <netinet/in_var.h>
#include <netinet/ip_var.h>
#include <netinet/tcp.h>
#include <netinet/tcp_fsm.h>
#include <netinet/tcp_seq.h>
#include <netinet/tcp_timer.h>
#include <netinet/tcp_var.h>
#include "rump_shim.h"

static void
tcp_sin(struct sockaddr_in *sin, uint16_t port)
{

	memset(sin, 0, sizeof(*sin));
	sin->sin_len = sizeof(*sin);
	sin->sin_family = AF_INET;
	sin->sin_port = htons(port);
	sin->sin_addr.s_addr = htonl(INADDR_LOOPBACK);
}

int
rump_tcp_pair(uint16_t cport, uint16_t sport, struct socket **clp,
    struct socket **cp, struct socket **sp, struct lwp *l)
{
	struct sockaddr_in sin_c, sin_s;
	struct socket *cl, *c;
	int error;

	if (socreate(AF_INET, &cl, SOCK_STREAM, 0, l, NULL) != 0)
		return -1;
	tcp_sin(&sin_s, sport);
	if (sobind(cl, (struct sockaddr *)&sin_s, l) != 0 ||
	    solisten(cl, 4, l) != 0)
		return -1;

	if (socreate(AF_INET, &c, SOCK_STREAM, 0, l, NULL) != 0)
		return -1;
	c->so_state |= SS_NBIO;
	tcp_sin(&sin_c, cport);
	if (sobind(c, (struct sockaddr *)&sin_c, l) != 0)
		return -1;
	solock(c);
	error = soconnect(c, (struct sockaddr *)&sin_s, l);
	sounlock(c);
	if (error != 0)
		return error;

	*clp = cl;
	*cp = c;
	*sp = NULL;
	return 0;
}

int
rump_tcp_connected(struct socket *so)
{
	struct tcpcb *tp;

	if (so->so_pcb == NULL)
		return 0;
	tp = intotcpcb(sotoinpcb(so));
	if (tp == NULL)
		return 0;
	return (so->so_state & SS_ISCONNECTED) != 0 &&
	    tp->t_state == TCPS_ESTABLISHED;
}

int
rump_tcp_accept(struct socket *cl, struct socket **sp)
{
	struct sockaddr_in peer;
	struct socket *so;
	int error;

	solock(cl);
	so = TAILQ_FIRST(&cl->so_q);
	if (so != NULL && !soqremque(so, 1))
		so = NULL;
	sounlock(cl);
	if (so == NULL)
		return 0;
	solock(so);
	/* tcp_accept() fills the peer address unconditionally: pass a real
	 * buffer (NULL wrote to address 0, which x86 mapped by accident). */
	memset(&peer, 0, sizeof(peer));
	error = soaccept(so, (struct sockaddr *)&peer);
	sounlock(so);
	if (error != 0)
		return -1;
	*sp = so;
	return 1;
}

void
rump_tcp_shutdown(struct socket *c, struct socket *s, int *cflag, int *sflag)
{

	if (!*cflag) {
		solock(c);
		(void)soshutdown(c, SHUT_WR);
		sounlock(c);
		*cflag = 1;
	}
	if (!*sflag && (s->so_state & SS_CANTRCVMORE) != 0) {
		solock(s);
		(void)soshutdown(s, SHUT_WR);
		sounlock(s);
		*sflag = 1;
	}
}

void
rump_tcp_close(struct socket **clp, struct socket **cp, struct socket **sp)
{

	soclose(*cp);
	soclose(*sp);
	soclose(*clp);
	*clp = *cp = *sp = NULL;
}
