/* rump_virtio_net_hw.h - virtio 1.x MMIO transport constants and the split
 * virtqueue layout used by the aarch64 virtio-net adapter (ours, M11 R9b).
 * Offsets verified against QEMU v10.2 include/standard-headers/linux/
 * virtio_mmio.h (same layout as drivers/c/include/virtio_mmio.h); ring
 * layout per the Virtio 1.2 specification (16-byte descriptors, 16-bit
 * avail/used indices).  QEMU `virt` exposes 32 slots at 0x0a000000 with a
 * 0x200-byte stride; modern (version 2) operation needs
 * `-global virtio-mmio.force-legacy=false`. */
#ifndef FANTUAN_RUMP_VIRTIO_NET_HW_H
#define FANTUAN_RUMP_VIRTIO_NET_HW_H

#include <stdint.h>

/* --- MMIO registers (virtio-mmio.h) -------------------------------------- */
#define VIRTIO_MMIO_MAGIC_VALUE		0x000
#define VIRTIO_MMIO_VERSION		0x004
#define VIRTIO_MMIO_DEVICE_ID		0x008
#define VIRTIO_MMIO_DEVICE_FEATURES	0x010
#define VIRTIO_MMIO_DEVICE_FEATURES_SEL	0x014
#define VIRTIO_MMIO_DRIVER_FEATURES	0x020
#define VIRTIO_MMIO_DRIVER_FEATURES_SEL	0x024
#define VIRTIO_MMIO_QUEUE_SEL		0x030
#define VIRTIO_MMIO_QUEUE_NUM_MAX	0x034
#define VIRTIO_MMIO_QUEUE_NUM		0x038
#define VIRTIO_MMIO_QUEUE_READY		0x044
#define VIRTIO_MMIO_QUEUE_NOTIFY		0x050
#define VIRTIO_MMIO_INTERRUPT_STATUS	0x060
#define VIRTIO_MMIO_INTERRUPT_ACK	0x064
#define VIRTIO_MMIO_STATUS		0x070
#define VIRTIO_MMIO_QUEUE_DESC_LOW	0x080
#define VIRTIO_MMIO_QUEUE_DESC_HIGH	0x084
#define VIRTIO_MMIO_QUEUE_AVAIL_LOW	0x090
#define VIRTIO_MMIO_QUEUE_AVAIL_HIGH	0x094
#define VIRTIO_MMIO_QUEUE_USED_LOW	0x0a0
#define VIRTIO_MMIO_QUEUE_USED_HIGH	0x0a4
#define VIRTIO_MMIO_CONFIG		0x100

#define VIRTIO_MAGIC		0x74726976u	/* "virt" */
#define VIRTIO_VERSION_MODERN	2u
#define VIRTIO_ID_NET		1u

#define VIRTIO_STATUS_ACK		1u
#define VIRTIO_STATUS_DRIVER		2u
#define VIRTIO_STATUS_DRIVER_OK		4u
#define VIRTIO_STATUS_FEATURES_OK	8u
#define VIRTIO_STATUS_FAILED		0x80u

/* Feature bits: VIRTIO_F_VERSION_1 is bit 32 (high word bit 0); the net
 * features we accept are MAC (5) and STATUS (16).  Checksum/TSO/MRG_RXBUF
 * stay off, so the stack keeps software checksums and full-frame RX. */
#define VIRTIO_F_VERSION_1_HI	1u
#define VIRTIO_NET_F_MAC	(1u << 5)
#define VIRTIO_NET_F_STATUS	(1u << 16)

#define VIRTIO_MMIO_BASE	0x0a000000ull
#define VIRTIO_MMIO_STRIDE	0x200u
#define VIRTIO_MMIO_SLOTS	32u

/* --- virtio-net queues and buffers --------------------------------------- */
#define VNET_QUEUE_SIZE	16u
#define VNET_RX_QUEUE	0u
#define VNET_TX_QUEUE	1u
#define VNET_BUF_SIZE	2048u
/* struct virtio_net_hdr_mrg_rxbuf: with VIRTIO_F_VERSION_1 negotiated the
 * header always carries the num_buffers field (QEMU uses 12 bytes for
 * version-1 devices even without VIRTIO_NET_F_MRG_RXBUF). */
#define VNET_HDR_SIZE	12u

/* --- split virtqueue (Virtio 1.2 section 2.7) ---------------------------- */
#define VRING_DESC_F_NEXT	1u
#define VRING_DESC_F_WRITE	2u

struct vring_desc {
	uint64_t addr;
	uint32_t len;
	uint16_t flags;
	uint16_t next;
};

struct vring_avail {
	uint16_t flags;
	uint16_t idx;
	uint16_t ring[VNET_QUEUE_SIZE];
	uint16_t used_event;
};

struct vring_used_elem {
	uint32_t id;
	uint32_t len;
};

struct vring_used {
	uint16_t flags;
	uint16_t idx;
	struct vring_used_elem ring[VNET_QUEUE_SIZE];
	uint16_t avail_event;
};

#endif /* FANTUAN_RUMP_VIRTIO_NET_HW_H */
