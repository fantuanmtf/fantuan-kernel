/* rump_virtio_net_var.h - shared state of the virtio-net adapter (ours,
 * M11 R9b).  The transport/probe half (rump_virtio_net.c) and the queue
 * half (rump_virtio_net_dma.c) share the softc and the avail-ring helpers;
 * split for the 300-line rule. */
#ifndef FANTUAN_RUMP_VIRTIO_NET_VAR_H
#define FANTUAN_RUMP_VIRTIO_NET_VAR_H

#include <sys/types.h>
#include <net/if_ether.h>
#include "rump_virtio_net_hw.h"

struct vnet_queue {
	volatile struct vring_desc *desc;
	volatile struct vring_avail *avail;
	volatile struct vring_used *used;
	uint64_t ring_phys;
};

struct vnet_softc {
	uint8_t *regs;
	uint8_t mac[ETHER_ADDR_LEN];
	int has_status;
	struct vnet_queue rx, tx;
	uint8_t *rx_bufs;
	uint64_t rx_bufs_phys;
	uint8_t *tx_bufs;
	uint64_t tx_bufs_phys;
	uint16_t tx_head;
	uint16_t tx_done;
	uint16_t rx_done;
	int ok;
};

extern struct vnet_softc vnet_sc;

void vnet_fence(void);
void vnet_notify(uint32_t sel);
void vnet_avail_add(struct vnet_queue *q, uint32_t idx, uint32_t sel);

#endif /* FANTUAN_RUMP_VIRTIO_NET_VAR_H */
