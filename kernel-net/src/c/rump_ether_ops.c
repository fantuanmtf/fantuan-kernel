/* rump_ether_ops.c - the shared ethernet net_ops table (ours, M11 R9b).
 * Split from rump_ether_if.c for the 300-line rule; the ops reach the
 * hardware through rump_ether_hw() (set by rump_ether_up). net_ifattach()
 * rebinds priv to the rump ifnet, so the ops take the ifnet as priv. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <net/if.h>
#include "rump_shim.h"
#include "rump_ether_if.h"

static int
ether_op_init(void *priv)
{
	struct ifnet *ifp = priv;

	if (ifp == NULL || (ifp->if_flags & IFF_RUNNING) == 0)
		return -1;
	return 0;
}

static void
ether_op_mac(void *priv, uint8_t out[6])
{

	(void)priv;
	rump_ether_hw()->mac(out);
}

static int
ether_op_send(void *priv, const void *frame, size_t len)
{

	(void)priv;
	return rump_ether_hw()->send(frame, len);
}

static int
ether_op_recv(void *priv, void *frame, size_t max)
{

	(void)priv;
	return rump_ether_hw()->recv(frame, max);
}

static int
ether_op_link(void *priv)
{
	struct ifnet *ifp = priv;

	return ifp != NULL && (ifp->if_flags & IFF_RUNNING) != 0 &&
	    rump_ether_hw()->link();
}

static const struct net_ops ether_net_ops = {
	.name = "ether",
	.init = ether_op_init,
	.mac = ether_op_mac,
	.send = ether_op_send,
	.recv = ether_op_recv,
	.link = ether_op_link,
};

const struct net_ops *
rump_ether_ops(void)
{

	return &ether_net_ops;
}
