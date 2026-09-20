/* rump_virtio_net_dma.c - virtio-net RX/TX queue paths (ours, M11 R9b).
 * Split from rump_virtio_net.c for the 300-line rule.  One descriptor per
 * frame, full-frame mode (no MRG_RXBUF): TX copies the frame behind the
 * 12-byte modern header into a ring buffer, RX recycles completed buffers.
 * Polled from the net task, like the e1000. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include "rump_shim.h"
#include "rump_ether_if.h"
#include "rump_virtio_net.h"
#include "rump_virtio_net_hw.h"
#include "rump_virtio_net_var.h"

struct vnet_softc vnet_sc;

/* Reclaim completed TX descriptors (in-order completion). */
void
vnet_hw_tx_reclaim(void)
{

	if (!vnet_sc.ok)
		return;
	vnet_fence();
	while (vnet_sc.tx_done != vnet_sc.tx.used->idx)
		vnet_sc.tx_done++;
}

int
vnet_hw_send(const void *frame, size_t len)
{
	uint16_t slot;
	uint8_t *buf;

	if (!vnet_sc.ok || frame == NULL || len == 0 ||
	    len > VNET_BUF_SIZE - VNET_HDR_SIZE)
		return -1;
	vnet_hw_tx_reclaim();
	if ((uint16_t)(vnet_sc.tx_head - vnet_sc.tx_done) >= VNET_QUEUE_SIZE)
		return -1;		/* ring full: drop, like a real NIC */
	slot = vnet_sc.tx_head % VNET_QUEUE_SIZE;
	buf = vnet_sc.tx_bufs + (uint64_t)slot * VNET_BUF_SIZE;
	memset(buf, 0, VNET_HDR_SIZE);
	memcpy(buf + VNET_HDR_SIZE, frame, len);
	vnet_sc.tx.desc[slot].addr =
	    vnet_sc.tx_bufs_phys + (uint64_t)slot * VNET_BUF_SIZE;
	vnet_sc.tx.desc[slot].len = VNET_HDR_SIZE + (uint32_t)len;
	vnet_sc.tx.desc[slot].flags = 0;
	vnet_sc.tx.desc[slot].next = 0;
	vnet_avail_add(&vnet_sc.tx, slot, VNET_TX_QUEUE);
	vnet_sc.tx_head++;
	return (int)len;
}

int
vnet_hw_recv(void *frame, size_t max)
{
	struct vring_used_elem elem;
	uint16_t avail;
	uint32_t id, n;

	if (!vnet_sc.ok)
		return -1;
	vnet_fence();
	if (vnet_sc.rx.used->idx == vnet_sc.rx_done)
		return 0;	/* no completed buffer since the last call */
	elem = vnet_sc.rx.used->ring[vnet_sc.rx_done % VNET_QUEUE_SIZE];
	vnet_sc.rx_done++;
	id = elem.id;
	if (id >= VNET_QUEUE_SIZE || elem.len <= VNET_HDR_SIZE)
		return -1;
	n = elem.len - VNET_HDR_SIZE;
	if (n > max)
		n = (uint32_t)max;
	memcpy(frame, vnet_sc.rx_bufs + (uint64_t)id * VNET_BUF_SIZE +
	    VNET_HDR_SIZE, n);
	/* Recycle the buffer: avail->idx is the local publish count, which
	 * starts at VNET_QUEUE_SIZE after vnet_rx_fill(). */
	avail = vnet_sc.rx.avail->idx;
	vnet_sc.rx.avail->ring[avail % VNET_QUEUE_SIZE] = (uint16_t)id;
	vnet_fence();
	vnet_sc.rx.avail->idx = avail + 1;
	vnet_fence();
	vnet_notify(VNET_RX_QUEUE);
	return (int)n;
}
