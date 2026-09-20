/* rump_e1000_if.c - e1000 glue onto the shared ethernet ifnet core (ours,
 * M11 R6; R9b split).  The PCI/MMIO/DMA hardware lives in rump_e1000.c and
 * rump_e1000_dma.c; this file only binds it to struct rump_ether_hw and
 * exports the arch-neutral rump_nic contract (rump_nic.h). */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include "rump_shim.h"
#include "rump_nic.h"
#include "rump_ether_if.h"
#include "rump_e1000.h"

static const struct rump_ether_hw e1000_hw_ops = {
	.init = e1000_hw_init,
	.mac = e1000_hw_mac,
	.send = e1000_hw_send,
	.recv = e1000_hw_recv,
	.tx_reclaim = e1000_hw_tx_reclaim,
	.link = e1000_hw_link,
};

int
rump_nic_up(void)
{

	return rump_ether_up("e1000", &e1000_hw_ops);
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

	return "e1000";
}

void
rump_nic_counters(unsigned long long *in, unsigned long long *out)
{

	rump_ether_counters(in, out);
}
