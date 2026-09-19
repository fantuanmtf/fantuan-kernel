/* rump_shim_route.c - route/kauth/module services for the R3 loopback path
 * (ours).  The MI route table (route.c, radix.c, rtbl.c) and the ifnet core
 * are real NetBSD files; this file fills the services the loopback path
 * never calls in anger and stores the MODULE_HOOK() variables that
 * compat_stub.c defines in a full NetBSD build.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/mbuf.h>
#include <sys/errno.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <sys/kernel.h>
#include <sys/kauth.h>
#include <sys/localcount.h>
#include <sys/module.h>
#include <sys/module_hook.h>
#include <sys/compat_stub.h>
#include <sys/sched.h>
#include <net/if.h>
#include <net/if_llatbl.h>
#include <net/route.h>
#include <netinet/in_offload.h>
#include "rump_shim.h"

const int schedppq = 1;

int ip_do_loopback_cksum;
int tcp_do_loopback_cksum;
int udp_do_loopback_cksum;
bool icmp_dynamic_rt_msg;

/* MODULE_HOOK() storage: compat_stub.c in a full NetBSD build. */
struct if_cvtcmd_43_hook_t if_cvtcmd_43_hook;
struct if_ifioctl_43_hook_t if_ifioctl_43_hook;
struct ifmedia_80_pre_hook_t ifmedia_80_pre_hook;
struct ifmedia_80_post_hook_t ifmedia_80_post_hook;
struct uipc_syscalls_40_hook_t uipc_syscalls_40_hook;
struct uipc_syscalls_50_hook_t uipc_syscalls_50_hook;

/* Installed by ifinit(); rtsock_shared.c supplies the real one upstream. */
int (*ifioctl)(struct socket *, u_long, void *, struct lwp *);

int
enosys(void)
{

	return ENOSYS;
}

int
kauth_authorize_network(kauth_cred_t cred, kauth_action_t action,
    enum kauth_network_req req, void *arg0, void *arg1, void *arg2)
{

	(void)cred;
	(void)action;
	(void)req;
	(void)arg0;
	(void)arg1;
	(void)arg2;
	return KAUTH_RESULT_DEFER;
}

kauth_cred_t
kauth_cred_get(void)
{

	return NULL;
}

kauth_listener_t
kauth_listen_scope(const char *name, kauth_scope_callback_t cb, void *arg)
{

	(void)name;
	(void)cb;
	(void)arg;
	return NULL;
}

int
module_autoload(const char *name, modclass_t cls)
{

	(void)name;
	(void)cls;
	return 0;
}

bool
module_hook_tryenter(bool *hooked, struct localcount *lc)
{

	(void)hooked;
	(void)lc;
	return false;
}

void
module_hook_exit(struct localcount *lc)
{

	(void)lc;
}

void
rt_ifannouncemsg(struct ifnet *ifp, int what)
{

	(void)ifp;
	(void)what;
}

void
rt_ifmsg(struct ifnet *ifp)
{

	(void)ifp;
}

void
rt_missmsg(int type, const struct rt_addrinfo *info, int flags, int error)
{

	(void)type;
	(void)info;
	(void)flags;
	(void)error;
}

void
rt_addrmsg(int type, struct ifaddr *ifa)
{

	(void)type;
	(void)ifa;
}

void
rt_addrmsg_rt(int type, struct ifaddr *ifa, int error, struct rtentry *rt)
{

	(void)type;
	(void)ifa;
	(void)error;
	(void)rt;
}

void
rt_setmetrics(void *m, struct rtentry *rt)
{

	(void)m;
	(void)rt;
}

void
lltable_prefix_free(const int af, const struct sockaddr *dst,
    const struct sockaddr *mask, const u_int flags)
{

	(void)af;
	(void)dst;
	(void)mask;
	(void)flags;
}

void
encapinit(void)
{
}

void
in_undefer_cksum(struct mbuf *m, size_t off, int flags)
{

	(void)m;
	(void)off;
	(void)flags;
	panic("in_undefer_cksum: hardware offload not supported (R3)");
}
