/* rump_shim_net.c - interface registry storage and mbuf tunables (ours).
 * No interface exists until R3; the registry is empty and if_acquire/if_put
 * only fill the psref so the mbuf drain path stays link-complete.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/mbuf.h>
#include <sys/domain.h>
#include <sys/pslist.h>
#include <sys/psref.h>
#include <sys/lwp.h>
#include <net/if.h>
#include "rump_shim.h"

struct domainhead domains = STAILQ_HEAD_INITIALIZER(domains);
struct pslist_head ifnet_pslist = { .plh_first = NULL };

const int msize = MSIZE;
const int mclbytes = MCLBYTES;
int nmbclusters;
int mblowat = 1;
int mcllowat = 1;

void
if_acquire(struct ifnet *ifp, struct psref *psref)
{

	memset(psref, 0, sizeof(*psref));
	psref->psref_target = (const struct psref_target *)&ifp->if_psref;
	psref->psref_lwp = curlwp;
	psref->psref_cpu = curcpu();
}

void
if_put(const struct ifnet *ifp, struct psref *psref)
{

	(void)ifp;
	memset(psref, 0, sizeof(*psref));
}
