/* rump_virtio_net.c - polled virtio-net over the modern (1.x) MMIO
 * transport for aarch64 QEMU `virt` (ours, M11 R9b).  The adapter C is
 * arch-neutral: the MMIO window, the descriptor rings and the packet
 * buffers all come through the kernel-net Env hooks
 * (fantuan_rump_mmio_map, fantuan_rump_pages_alloc/virt_to_phys), so the
 * code only depends on physical addresses being device-visible.
 *
 * Design: scan the 32 QEMU virt slots at 0x0a000000+0x200*n for a modern
 * (version 2) net device, negotiate VIRTIO_F_VERSION_1 plus MAC/STATUS,
 * set up one RX and one TX split virtqueue (16 descriptors each) and run
 * them polled from the net task - the same contract as the e1000.  Only
 * the full-frame mode is used (no MRG_RXBUF), checksum/TSO offloads stay
 * unnegotiated so the stack keeps software checksums.  QEMU DMA is
 * cache-coherent with the CPU; real hardware would need cache maintenance
 * (documented in ADAPTATION.md).  The ifnet half is rump_virtio_net_if.c;
 * the transport constants are in rump_virtio_net_hw.h. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include <net/if_ether.h>
#include "rump_shim.h"
#include "rump_ether_if.h"
#include "rump_virtio_net.h"
#include "rump_virtio_net_hw.h"

#include "rump_virtio_net_var.h"

static uint32_t
vnet_rd32(uint32_t off)
{

	return *(volatile uint32_t *)(uintptr_t)(vnet_sc.regs + off);
}

static void
vnet_wr32(uint32_t off, uint32_t v)
{

	*(volatile uint32_t *)(uintptr_t)(vnet_sc.regs + off) = v;
}

void
vnet_fence(void)
{

	__atomic_thread_fence(__ATOMIC_SEQ_CST);
}

/* Add descriptor IDX to the queue's avail ring and notify the device. */
void
vnet_avail_add(struct vnet_queue *q, uint32_t idx, uint32_t sel)
{
	uint16_t a = q->avail->idx;

	q->avail->ring[a % VNET_QUEUE_SIZE] = (uint16_t)idx;
	vnet_fence();
	q->avail->idx = a + 1;
	vnet_fence();
	vnet_notify(sel);
}

void
vnet_notify(uint32_t sel)
{

	vnet_wr32(VIRTIO_MMIO_QUEUE_NOTIFY, sel);
}

static int
vnet_queue_init(struct vnet_queue *q, uint32_t sel_reg)
{
	uint8_t *ring;

	vnet_wr32(VIRTIO_MMIO_QUEUE_SEL, sel_reg);
	if (vnet_rd32(VIRTIO_MMIO_QUEUE_NUM_MAX) < VNET_QUEUE_SIZE)
		return EINVAL;
	vnet_wr32(VIRTIO_MMIO_QUEUE_NUM, VNET_QUEUE_SIZE);
	ring = fantuan_rump_pages_alloc(1);
	if (ring == NULL)
		return ENOMEM;
	q->ring_phys = fantuan_rump_virt_to_phys(ring);
	memset(ring, 0, 4096);
	q->desc = (volatile struct vring_desc *)ring;
	q->avail = (volatile struct vring_avail *)(ring + 0x100);
	q->used = (volatile struct vring_used *)(ring + 0x200);
	vnet_wr32(VIRTIO_MMIO_QUEUE_DESC_LOW, (uint32_t)q->ring_phys);
	vnet_wr32(VIRTIO_MMIO_QUEUE_DESC_HIGH, (uint32_t)(q->ring_phys >> 32));
	vnet_wr32(VIRTIO_MMIO_QUEUE_AVAIL_LOW, (uint32_t)(q->ring_phys + 0x100));
	vnet_wr32(VIRTIO_MMIO_QUEUE_AVAIL_HIGH,
	    (uint32_t)((q->ring_phys + 0x100) >> 32));
	vnet_wr32(VIRTIO_MMIO_QUEUE_USED_LOW, (uint32_t)(q->ring_phys + 0x200));
	vnet_wr32(VIRTIO_MMIO_QUEUE_USED_HIGH,
	    (uint32_t)((q->ring_phys + 0x200) >> 32));
	vnet_wr32(VIRTIO_MMIO_QUEUE_READY, 1);
	vnet_fence();
	return 0;
}

/* Fill every RX descriptor with a full-frame buffer and publish the ring. */
static void
vnet_rx_fill(void)
{
	uint16_t i;

	for (i = 0; i < VNET_QUEUE_SIZE; i++) {
		vnet_sc.rx.desc[i].addr =
		    vnet_sc.rx_bufs_phys + (uint64_t)i * VNET_BUF_SIZE;
		vnet_sc.rx.desc[i].len = VNET_BUF_SIZE;
		vnet_sc.rx.desc[i].flags = VRING_DESC_F_WRITE;
		vnet_sc.rx.desc[i].next = 0;
		vnet_sc.rx.avail->ring[i] = i;
	}
	vnet_fence();
	vnet_sc.rx.avail->idx = VNET_QUEUE_SIZE;
	vnet_fence();
	vnet_wr32(VIRTIO_MMIO_QUEUE_NOTIFY, VNET_RX_QUEUE);
}

static int
vnet_probe_slot(uint64_t pa)
{
	uint32_t lo, hi, status;
	uint8_t *base;
	int i;

	base = fantuan_rump_mmio_map(pa, VIRTIO_MMIO_STRIDE);
	if (base == NULL)
		return ENXIO;
	if (*(volatile uint32_t *)(uintptr_t)(base + VIRTIO_MMIO_MAGIC_VALUE)
	    != VIRTIO_MAGIC)
		return ENXIO;
	if (*(volatile uint32_t *)(uintptr_t)(base + VIRTIO_MMIO_VERSION)
	    != VIRTIO_VERSION_MODERN)
		return ENXIO;
	if (*(volatile uint32_t *)(uintptr_t)(base + VIRTIO_MMIO_DEVICE_ID)
	    != VIRTIO_ID_NET)
		return ENXIO;
	vnet_sc.regs = base;

	vnet_wr32(VIRTIO_MMIO_STATUS, VIRTIO_STATUS_ACK);
	vnet_wr32(VIRTIO_MMIO_STATUS,
	    VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER);
	vnet_wr32(VIRTIO_MMIO_DEVICE_FEATURES_SEL, 0);
	lo = vnet_rd32(VIRTIO_MMIO_DEVICE_FEATURES);
	vnet_wr32(VIRTIO_MMIO_DEVICE_FEATURES_SEL, 1);
	hi = vnet_rd32(VIRTIO_MMIO_DEVICE_FEATURES);
	if ((hi & VIRTIO_F_VERSION_1_HI) == 0) {
		printf("net: virtio-net not version 1\n");
		return ENXIO;
	}
	vnet_wr32(VIRTIO_MMIO_DRIVER_FEATURES_SEL, 0);
	vnet_wr32(VIRTIO_MMIO_DRIVER_FEATURES,
	    lo & (VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS));
	vnet_wr32(VIRTIO_MMIO_DRIVER_FEATURES_SEL, 1);
	vnet_wr32(VIRTIO_MMIO_DRIVER_FEATURES, VIRTIO_F_VERSION_1_HI);
	vnet_wr32(VIRTIO_MMIO_STATUS, VIRTIO_STATUS_ACK |
	    VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK);
	if ((vnet_rd32(VIRTIO_MMIO_STATUS) & VIRTIO_STATUS_FEATURES_OK) == 0) {
		printf("net: virtio-net feature negotiation rejected\n");
		vnet_wr32(VIRTIO_MMIO_STATUS, VIRTIO_STATUS_FAILED);
		return ENXIO;
	}

	memset(vnet_sc.mac, 0, sizeof(vnet_sc.mac));
	if ((lo & VIRTIO_NET_F_MAC) != 0) {
		for (i = 0; i < ETHER_ADDR_LEN; i++)
			vnet_sc.mac[i] = *(volatile uint8_t *)(uintptr_t)
			    (vnet_sc.regs + VIRTIO_MMIO_CONFIG + i);
	}
	vnet_sc.has_status = (lo & VIRTIO_NET_F_STATUS) != 0;

	vnet_sc.rx_bufs = fantuan_rump_pages_alloc(VNET_QUEUE_SIZE *
	    VNET_BUF_SIZE / 4096);
	vnet_sc.tx_bufs = fantuan_rump_pages_alloc(VNET_QUEUE_SIZE *
	    VNET_BUF_SIZE / 4096);
	if (vnet_sc.rx_bufs == NULL || vnet_sc.tx_bufs == NULL)
		return ENOMEM;
	vnet_sc.rx_bufs_phys = fantuan_rump_virt_to_phys(vnet_sc.rx_bufs);
	vnet_sc.tx_bufs_phys = fantuan_rump_virt_to_phys(vnet_sc.tx_bufs);
	if (vnet_queue_init(&vnet_sc.rx, VNET_RX_QUEUE) != 0 ||
	    vnet_queue_init(&vnet_sc.tx, VNET_TX_QUEUE) != 0)
		return ENOMEM;

	vnet_sc.tx_head = vnet_sc.tx_done = vnet_sc.rx_done = 0;
	vnet_rx_fill();

	status = VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER |
	    VIRTIO_STATUS_FEATURES_OK | VIRTIO_STATUS_DRIVER_OK;
	vnet_wr32(VIRTIO_MMIO_STATUS, status);
	vnet_sc.ok = 1;
	return 0;
}

int
vnet_hw_init(void)
{
	uint32_t i;
	int error;

	if (vnet_sc.ok)
		return 0;
	for (i = 0; i < VIRTIO_MMIO_SLOTS; i++) {
		error = vnet_probe_slot(VIRTIO_MMIO_BASE +
		    (uint64_t)i * VIRTIO_MMIO_STRIDE);
		if (error == 0)
			return 0;
	}
	return ENXIO;
}

void
vnet_hw_mac(uint8_t out[ETHER_ADDR_LEN])
{

	memcpy(out, vnet_sc.mac, ETHER_ADDR_LEN);
}

int
vnet_hw_link(void)
{

	if (!vnet_sc.ok)
		return 0;
	if (!vnet_sc.has_status)
		return 1;
	return (*(volatile uint16_t *)(uintptr_t)
	    (vnet_sc.regs + VIRTIO_MMIO_CONFIG + 6) & 1) != 0;
}
