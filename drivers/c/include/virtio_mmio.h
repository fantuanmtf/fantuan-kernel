/* virtio_mmio.h — virtio 1.x MMIO register map and split-virtqueue layout
 * used by drivers/c/virtio_mmio.c. Offsets verified against QEMU v10.2
 * include/standard-headers/linux/virtio_mmio.h; ring layout per the Virtio
 * 1.2 specification (desc 16B entries, 16-bit avail/used indices). */
#ifndef FANTUAN_VIRTIO_MMIO_H
#define FANTUAN_VIRTIO_MMIO_H

#include <stdint.h>

/* --- MMIO registers (virtio-mmio.h) -------------------------------------- */
#define VIRTIO_MMIO_MAGIC_VALUE        0x000
#define VIRTIO_MMIO_VERSION            0x004
#define VIRTIO_MMIO_DEVICE_ID          0x008
#define VIRTIO_MMIO_DEVICE_FEATURES    0x010
#define VIRTIO_MMIO_DEVICE_FEATURES_SEL 0x014
#define VIRTIO_MMIO_DRIVER_FEATURES    0x020
#define VIRTIO_MMIO_DRIVER_FEATURES_SEL 0x024
#define VIRTIO_MMIO_QUEUE_SEL          0x030
#define VIRTIO_MMIO_QUEUE_NUM_MAX      0x034
#define VIRTIO_MMIO_QUEUE_NUM          0x038
#define VIRTIO_MMIO_QUEUE_READY        0x044
#define VIRTIO_MMIO_QUEUE_NOTIFY       0x050
#define VIRTIO_MMIO_INTERRUPT_STATUS   0x060
#define VIRTIO_MMIO_INTERRUPT_ACK      0x064
#define VIRTIO_MMIO_STATUS             0x070
#define VIRTIO_MMIO_QUEUE_DESC_LOW     0x080
#define VIRTIO_MMIO_QUEUE_DESC_HIGH    0x084
#define VIRTIO_MMIO_QUEUE_AVAIL_LOW    0x090
#define VIRTIO_MMIO_QUEUE_AVAIL_HIGH   0x094
#define VIRTIO_MMIO_QUEUE_USED_LOW     0x0a0
#define VIRTIO_MMIO_QUEUE_USED_HIGH    0x0a4
#define VIRTIO_MMIO_CONFIG             0x100

#define VIRTIO_MAGIC        0x74726976u /* "virt" */
#define VIRTIO_VERSION_MODERN 2u
#define VIRTIO_ID_BLOCK     2u

#define VIRTIO_STATUS_ACK          1u
#define VIRTIO_STATUS_DRIVER       2u
#define VIRTIO_STATUS_FEATURES_OK  8u
#define VIRTIO_STATUS_DRIVER_OK    4u
#define VIRTIO_STATUS_FAILED       0x80u

#define VIRTIO_BLK_T_IN   0u
#define VIRTIO_BLK_T_OUT  1u

#define VIRTIO_MMIO_SLOTS 8u
#define VIRTIO_MMIO_STRIDE 0x1000u

/* --- split virtqueue ------------------------------------------------------ */
#define QUEUE_SIZE 8u
#define VRING_DESC_F_NEXT  1u
#define VRING_DESC_F_WRITE 2u

struct vring_desc {
    uint64_t addr;
    uint32_t len;
    uint16_t flags;
    uint16_t next;
};

struct vring_avail {
    uint16_t flags;
    uint16_t idx;
    uint16_t ring[QUEUE_SIZE];
    uint16_t used_event;
};

struct vring_used_elem {
    uint32_t id;
    uint32_t len;
};

struct vring_used {
    uint16_t flags;
    uint16_t idx;
    struct vring_used_elem ring[QUEUE_SIZE];
    uint16_t avail_event;
};


#endif /* FANTUAN_VIRTIO_MMIO_H */
