/* rump_domain.c - protocol-domain list and lookup helpers (ours).  The real
 * NetBSD versions live in sys/kern/uipc_domain.c, which is not imported: it
 * drags the descriptor/compat/unpcb layer into the link.  The slice keeps
 * the domain list and the three lookup paths the socket/TCP code uses. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/mbuf.h>
#include <sys/domain.h>
#include <sys/protosw.h>
#include <sys/socket.h>
#include "rump_shim.h"

struct domainhead domains = STAILQ_HEAD_INITIALIZER(domains);

struct domain *
pffinddomain(int family)
{
	struct domain *dp;

	DOMAIN_FOREACH(dp) {
		if (dp->dom_family == family)
			return dp;
	}
	return NULL;
}

const struct protosw *
pffindtype(int family, int type)
{
	struct domain *dp;
	const struct protosw *pr;

	dp = pffinddomain(family);
	if (dp == NULL)
		return NULL;
	for (pr = dp->dom_protosw; pr < dp->dom_protoswNPROTOSW; pr++)
		if (pr->pr_type && pr->pr_type == type)
			return pr;
	return NULL;
}

const struct protosw *
pffindproto(int family, int proto, int type)
{
	struct domain *dp;
	const struct protosw *pr;
	const struct protosw *maybe = NULL;

	if (family == 0)
		return NULL;
	dp = pffinddomain(family);
	if (dp == NULL)
		return NULL;
	for (pr = dp->dom_protosw; pr < dp->dom_protoswNPROTOSW; pr++) {
		if (pr->pr_protocol == proto && pr->pr_type == type)
			return pr;
		if (type == SOCK_RAW && pr->pr_type == SOCK_RAW &&
		    pr->pr_protocol == 0 && maybe == NULL)
			maybe = pr;
	}
	return maybe;
}

void
pfctlinput(int cmd, const struct sockaddr *sa)
{
	struct domain *dp;
	const struct protosw *pr;

	DOMAIN_FOREACH(dp) {
		for (pr = dp->dom_protosw; pr < dp->dom_protoswNPROTOSW; pr++) {
			if (pr->pr_ctlinput != NULL)
				(void)pr->pr_ctlinput(cmd, sa, NULL);
		}
	}
}
