/* rump_e1000_ops.c - the e1000 driver's net_ops table (ours).
 * The registry binds priv to the rump ifnet (net_ifattach), so every op
 * takes the ifnet and reaches the hardware through the global softc. */

#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <net/if.h>
#include "rump_shim.h"
#include "rump_e1000.h"

static int
e1000_op_init(void *priv)
{
	struct ifnet *ifp = priv;

	if (ifp == NULL || (ifp->if_flags & IFF_RUNNING) == 0)
		return -1;
	return 0;
}

static void
e1000_op_mac(void *priv, uint8_t out[6])
{

	(void)priv;
	e1000_hw_mac(out);
}

static int
e1000_op_send(void *priv, const void *frame, size_t len)
{

	(void)priv;
	return e1000_hw_send(frame, len);
}

static int
e1000_op_recv(void *priv, void *frame, size_t max)
{

	(void)priv;
	return e1000_hw_recv(frame, max);
}

static int
e1000_op_link(void *priv)
{
	struct ifnet *ifp = priv;

	return ifp != NULL && (ifp->if_flags & IFF_RUNNING) != 0 &&
	    e1000_hw_link();
}

const struct net_ops e1000_net_ops = {
	.name = "e1000",
	.init = e1000_op_init,
	.mac = e1000_op_mac,
	.send = e1000_op_send,
	.recv = e1000_op_recv,
	.link = e1000_op_link,
};
