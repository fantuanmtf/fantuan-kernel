/* rump_shim_inet.c - protocol services the R4 IPv4 path does not exercise
 * (ours).  Every function here is a documented stub for a subsystem that is
 * outside the R4 slice: IGMP (R6/R7), IP encapsulation (R6), the raw-IP
 * socket layer (R5), TCP (R5) and the ephemeral-port allocator (port 0 binds
 * only; the R4 tests bind explicit ports).  They exist so the imported
 * in_proto.c protocol switch is complete and the link is honest about what
 * is not implemented yet.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/mbuf.h>
#include <sys/errno.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <sys/protosw.h>
#include <net/if.h>
#include <netinet/in.h>
#include <netinet/in_var.h>
#include <netinet/igmp_var.h>
#include <netinet/ip_encap.h>
#include <netinet/portalgo.h>
#include <netinet/in_systm.h>
#include <netinet/ip.h>
#include <netinet/ip_var.h>
#include <netinet/tcp.h>
#include <netinet/tcp_seq.h>
#include <netinet/tcp_timer.h>
#include <netinet/tcp_var.h>
#include <netinet/tcp_vtw.h>
#include "rump_shim.h"

/* IGMP: joins are tracked by in.c only; no group reports are emitted. */
void
igmp_init(void)
{
}

int
igmp_joingroup(struct in_multi *inm)
{

	(void)inm;
	return 0;
}

void
igmp_leavegroup(struct in_multi *inm)
{

	(void)inm;
}

void
igmp_purgeif(struct ifnet *ifp)
{

	(void)ifp;
}

void
igmp_input(struct mbuf *m, int off, int proto)
{

	(void)off;
	(void)proto;
	m_freem(m);
}

void
igmp_fasttimo(void)
{
}

void
igmp_slowtimo(void)
{
}

/* IP-in-IP/GRE/... encapsulation: not configured (NGIF/NGRE are 0). */
void
encap_init(void)
{
}

void
encap4_input(struct mbuf *m, int off, int proto)
{

	(void)off;
	(void)proto;
	m_freem(m);
	panic("encap4_input: encapsulation not imported until R6");
}

/* Raw IP sockets: the protosw entries exist so the domain is complete, but
 * no raw socket can be created until R5 imports raw_ip.c.  rip_input is the
 * one live path: ICMP echo replies reach it from ip_icmp.c and are handed
 * to the boot ping client; everything else is dropped. */
void
rip_init(void)
{
}

void
rip_input(struct mbuf *m, int hlen, int proto)
{

	if (proto == IPPROTO_ICMP && rump_ping_rx(m) == 0)
		return;
	m_freem(m);
}

void *
rip_ctlinput(int cmd, const struct sockaddr *sa, void *v)
{

	(void)cmd;
	(void)sa;
	(void)v;
	return NULL;
}

int
rip_ctloutput(int op, struct socket *so, struct sockopt *sopt)
{

	(void)op;
	(void)so;
	(void)sopt;
	return EOPNOTSUPP;
}

const struct pr_usrreqs rip_usrreqs;

/* TCP vestigial TIME_WAIT (tcp_vtw.c) is not imported: the feature is off
 * by default (tcp4_vtw_enable == 0) and the sysctl that would enable it is
 * read-only, so vtw_add() reports "not added" and the remaining entry
 * points assert if they are ever reached. */
int
vtw_add(int af, struct tcpcb *tp)
{

	(void)af;
	(void)tp;
	return 0;
}

void
vtw_del(vtw_ctl_t *ctl, vtw_t *vtw)
{

	(void)ctl;
	(void)vtw;
	panic("vtw_del: vestigial TIME_WAIT is not imported");
}

void
vtw_restart(vestigial_inpcb_t *vp)
{

	(void)vp;
	panic("vtw_restart: vestigial TIME_WAIT is not imported");
}

int
vtw_earlyinit(void)
{

	return 0;
}

int
sysctl_tcp_vtw_enable(SYSCTLFN_ARGS)
{

	(void)name;
	(void)namelen;
	(void)oldp;
	(void)oldlenp;
	(void)newp;
	(void)newlen;
	(void)oname;
	(void)l;
	(void)rnode;
	return 0;
}

/* Ephemeral port allocation: only reached for port-0 binds, which the R4
 * tests do not use; a bind without an explicit port fails cleanly. */
int
portalgo_randport(uint16_t *port, struct inpcb *inp, kauth_cred_t cred)
{

	(void)port;
	(void)inp;
	(void)cred;
	return EADDRNOTAVAIL;
}

int
portalgo_algo_index_select(struct inpcb *inp, int idx)
{

	(void)inp;
	(void)idx;
	return 0;
}

int
sysctl_portalgo_available(SYSCTLFN_ARGS)
{

	(void)name;
	(void)namelen;
	(void)oldp;
	(void)oldlenp;
	(void)newp;
	(void)newlen;
	(void)oname;
	(void)l;
	(void)rnode;
	return 0;
}

int
sysctl_portalgo_selected4(SYSCTLFN_ARGS)
{

	return sysctl_portalgo_available(SYSCTLFN_CALL(rnode));
}

int
sysctl_portalgo_reserve4(SYSCTLFN_ARGS)
{

	return sysctl_portalgo_available(SYSCTLFN_CALL(rnode));
}

/* sysctl_net_inet_ip_ports() and sysctl_inpcblist() are provided by the
 * imported tcp_usrreq.c / in_pcb.c. */
