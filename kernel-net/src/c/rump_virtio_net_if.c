/* rump_virtio_net_if.c - virtio-net glue onto the shared ethernet ifnet
 * core (ours, M11 R9b).  The MMIO transport lives in rump_virtio_net.c;
 * this file only binds it to struct rump_ether_hw and exports the
 * arch-neutral rump_nic contract (rump_nic.h). */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include "rump_shim.h"
#include "rump_nic.h"
#include "rump_ether_if.h"
#include "rump_virtio_net.h"

static const struct rump_ether_hw vnet_hw_ops = {
	.init = vnet_hw_init,
	.mac = vnet_hw_mac,
	.send = vnet_hw_send,
	.recv = vnet_hw_recv,
	.tx_reclaim = vnet_hw_tx_reclaim,
	.link = vnet_hw_link,
};

int
rump_nic_up(void)
{

	return rump_ether_up("virtio-net", &vnet_hw_ops);
}

void
rump_nic_poll(void)
{

	rump_ether_poll();
}

int
rump_nic_ready(void)
{

	return rump_ether_ready();
}

struct ifnet *
rump_nic_ifp(void)
{

	return rump_ether_ifp();
}

void
rump_nic_mac(uint8_t out[6])
{

	rump_ether_mac(out);
}

const char *
rump_nic_name(void)
{

	return "virtio-net";
}

void
rump_nic_counters(unsigned long long *in, unsigned long long *out)
{

	rump_ether_counters(in, out);
}
