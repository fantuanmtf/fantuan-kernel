/* rump_sock2.c - socket-buffer and socket-state slice for the R4 UDP path
 * (ours).  The full socket layer (uipc_socket.c/uipc_socket2.c) is R5; the
 * R4 UDP test only needs the receive-path pieces udp4_sendup() calls:
 * sbappendaddr() queues one datagram (address record + payload) on so_rcv
 * using the real sockbuf inline helpers, and the remaining functions are the
 * socket-state/sockopt stubs that the not-yet-exercised send paths need to
 * link.  R5 replaces this file with the imported implementation.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/mbuf.h>
#include <sys/errno.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <netinet/in.h>
#include "rump_shim.h"

int
sbappendaddr(struct sockbuf *sb, const struct sockaddr *asa, struct mbuf *m0,
    struct mbuf *control)
{
	struct mbuf *m, *n, *nlast;
	int space, len;

	space = asa->sa_len;
	if (m0 != NULL) {
		if ((m0->m_flags & M_PKTHDR) == 0)
			panic("sbappendaddr: no header mbuf");
		space += m0->m_pkthdr.len;
	}
	for (n = control; n != NULL; n = n->m_next) {
		space += n->m_len;
		if (n->m_next == NULL)
			break;
	}
	if ((u_long)space > sbspace(sb))
		return 0;
	m = m_get(M_DONTWAIT, MT_SONAME);
	if (m == NULL)
		return 0;
	len = asa->sa_len;
	m->m_len = len;
	memcpy(mtod(m, void *), asa, len);
	if (n != NULL)
		n->m_next = m0;
	else
		control = m0;
	m->m_next = control;
	for (n = m; n->m_next != NULL; n = n->m_next)
		sballoc(sb, n);
	sballoc(sb, n);
	nlast = n;
	if (sb->sb_lastrecord != NULL)
		sb->sb_lastrecord->m_nextpkt = m;
	else
		sb->sb_mb = m;
	sb->sb_lastrecord = m;
	sb->sb_mbtail = nlast;
	return 1;
}

struct mbuf *
sbcreatecontrol(void *p, int size, int type, int level)
{
	struct cmsghdr *cm;
	struct mbuf *m;

	if (size < 0 || (size_t)size + CMSG_SPACE(0) > MHLEN)
		return NULL;
	m = m_get(M_DONTWAIT, MT_CONTROL);
	if (m == NULL)
		return NULL;
	cm = mtod(m, struct cmsghdr *);
	if (p != NULL)
		memcpy(CMSG_DATA(cm), p, size);
	cm->cmsg_len = CMSG_LEN(size);
	cm->cmsg_level = level;
	cm->cmsg_type = type;
	m->m_len = CMSG_SPACE(size);
	return m;
}

struct mbuf **
sbsavetimestamp(int type, struct mbuf **controlp)
{

	/* SO_TIMESTAMP control is not produced before the socket layer
	 * exists (R5); the caller tolerates a NULL result. */
	(void)type;
	if (controlp != NULL)
		*controlp = NULL;
	return controlp;
}

void
sowakeup(struct socket *so, struct sockbuf *sb, int events)
{

	(void)so;
	(void)sb;
	(void)events;
}

void
soroverflow(struct socket *so)
{

	so->so_rcv.sb_overflowed++;
}

void
socantsendmore(struct socket *so)
{

	so->so_state |= SS_CANTSENDMORE;
}

void
soisconnected(struct socket *so)
{

	so->so_state &= ~SS_ISCONNECTING;
	so->so_state |= SS_ISCONNECTED;
}

void
sofree(struct socket *so)
{

	/* R5 owns socket lifetime; the R4 tests allocate their two endpoints
	 * statically and never reach the destroy path. */
	if (so != NULL)
		so->so_pcb = NULL;
}

bool
solocked(const struct socket *so)
{

	(void)so;
	return true;
}

void
solockretry(struct socket *so, kmutex_t *lock)
{

	(void)so;
	(void)lock;
}

void
sosetlock(struct socket *so)
{

	(void)so;
}

int
soreserve(struct socket *so, u_long sndcc, u_long rcvcc)
{

	(void)so;
	(void)sndcc;
	(void)rcvcc;
	return 0;
}

int
sockopt_set(struct sockopt *sopt, const void *data, size_t size)
{

	(void)sopt;
	(void)data;
	(void)size;
	return EOPNOTSUPP;
}

int
sockopt_setint(struct sockopt *sopt, int val)
{

	(void)sopt;
	(void)val;
	return EOPNOTSUPP;
}

int
sockopt_get(const struct sockopt *sopt, void *data, size_t size)
{

	(void)sopt;
	(void)data;
	(void)size;
	return EOPNOTSUPP;
}

int
sockopt_getint(const struct sockopt *sopt, int *valp)
{

	(void)sopt;
	(void)valp;
	return EOPNOTSUPP;
}

int
sockopt_setmbuf(struct sockopt *sopt, struct mbuf *m)
{

	(void)sopt;
	(void)m;
	return EOPNOTSUPP;
}
