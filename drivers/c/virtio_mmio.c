/* virtio_mmio.c — virtio 1.x MMIO block driver (M9.4, RISC-V reference).
 *
 * Transport: Virtio 1.2 §4.2.2 MMIO register layout, offsets verified
 * against QEMU v10.2 include/standard-headers/linux/virtio_mmio.h; split
 * virtqueue ring layout per virtio_ring.h (desc 16B entries, avail/used
 * 16-bit indices). One queue, one request in flight, polled completion —
 * no PLIC wiring in v1. Data bounces through a 4 KiB DMA page in
 * 8-sector chunks, matching the AHCI driver's ownership model.
 *
 * The core passes the MMIO region as a virtual address (the kernel alias);
 * queue memory and the request page come from k_alloc_page so the device
 * sees guest-physical addresses.
 */

#include "driver.h"
#include "rust_core.h"

#include "virtio_mmio.h"

struct virtio_dev {
    uint64_t base;              /* MMIO virtual address */
    /* Ring memory is shared with the device: every access is volatile. */
    volatile struct vring_desc *desc; /* queue page */
    volatile struct vring_avail *avail;
    volatile struct vring_used *used;
    uint64_t queue_phys;
    uint8_t *req;               /* header(16) + status(1) page (virtual) */
    uint64_t req_phys;
    uint8_t *data;              /* 4 KiB bounce page (virtual) */
    uint64_t data_phys;
    uint64_t capacity;          /* 512-byte sectors */
};

static struct virtio_dev g_dev;

static inline uint32_t rd32(uint64_t base, uint32_t off)
{
    return *(volatile uint32_t *)(uintptr_t)(base + off);
}

static inline void wr32(uint64_t base, uint32_t off, uint32_t v)
{
    *(volatile uint32_t *)(uintptr_t)(base + off) = v;
}

static void fence(void)
{
    __atomic_thread_fence(__ATOMIC_SEQ_CST);
}

static void memzero(void *p, uint64_t n)
{
    uint8_t *b = p;
    while (n-- > 0) {
        *b++ = 0;
    }
}

static int queue_init(struct virtio_dev *d)
{
    uint64_t phys;
    uint32_t max;

    d->desc = k_alloc_page(&phys);
    if (d->desc == NULL) {
        return -1;
    }
    d->queue_phys = phys;
    memzero((void *)(uintptr_t)d->desc, 4096);
    d->avail = (volatile struct vring_avail *)((uintptr_t)d->desc + 0x100);
    d->used = (volatile struct vring_used *)((uintptr_t)d->desc + 0x200);

    wr32(d->base, VIRTIO_MMIO_QUEUE_SEL, 0);
    max = rd32(d->base, VIRTIO_MMIO_QUEUE_NUM_MAX);
    if (max < QUEUE_SIZE) {
        k_log("virtio: queue too small\n");
        return -1;
    }
    wr32(d->base, VIRTIO_MMIO_QUEUE_NUM, QUEUE_SIZE);
    wr32(d->base, VIRTIO_MMIO_QUEUE_DESC_LOW, (uint32_t)phys);
    wr32(d->base, VIRTIO_MMIO_QUEUE_DESC_HIGH, (uint32_t)(phys >> 32));
    wr32(d->base, VIRTIO_MMIO_QUEUE_AVAIL_LOW, (uint32_t)(phys + 0x100));
    wr32(d->base, VIRTIO_MMIO_QUEUE_AVAIL_HIGH, (uint32_t)((phys + 0x100) >> 32));
    wr32(d->base, VIRTIO_MMIO_QUEUE_USED_LOW, (uint32_t)(phys + 0x200));
    wr32(d->base, VIRTIO_MMIO_QUEUE_USED_HIGH, (uint32_t)((phys + 0x200) >> 32));
    wr32(d->base, VIRTIO_MMIO_QUEUE_READY, 1);
    fence();
    return 0;
}

static int dev_probe(uint64_t base)
{
    struct virtio_dev *d = &g_dev;
    uint32_t hi;

    if (rd32(base, VIRTIO_MMIO_MAGIC_VALUE) != VIRTIO_MAGIC ||
        rd32(base, VIRTIO_MMIO_VERSION) != VIRTIO_VERSION_MODERN ||
        rd32(base, VIRTIO_MMIO_DEVICE_ID) != VIRTIO_ID_BLOCK) {
        return -1;
    }
    d->base = base;

    wr32(base, VIRTIO_MMIO_STATUS, VIRTIO_STATUS_ACK);
    wr32(base, VIRTIO_MMIO_STATUS, VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER);

    /* Accept only VIRTIO_F_VERSION_1 (feature bit 32). */
    wr32(base, VIRTIO_MMIO_DEVICE_FEATURES_SEL, 1);
    hi = rd32(base, VIRTIO_MMIO_DEVICE_FEATURES);
    if ((hi & 1u) == 0) {
        k_log("virtio: device is not version 1\n");
        return -1;
    }
    wr32(base, VIRTIO_MMIO_DRIVER_FEATURES_SEL, 1);
    wr32(base, VIRTIO_MMIO_DRIVER_FEATURES, 1);
    wr32(base, VIRTIO_MMIO_DRIVER_FEATURES_SEL, 0);
    wr32(base, VIRTIO_MMIO_DRIVER_FEATURES, 0);
    wr32(base, VIRTIO_MMIO_STATUS,
         VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK);
    if ((rd32(base, VIRTIO_MMIO_STATUS) & VIRTIO_STATUS_FEATURES_OK) == 0) {
        k_log("virtio: feature negotiation rejected\n");
        wr32(base, VIRTIO_MMIO_STATUS, VIRTIO_STATUS_FAILED);
        return -1;
    }

    d->req = k_alloc_page(&d->req_phys);
    d->data = k_alloc_page(&d->data_phys);
    if (d->req == NULL || d->data == NULL) {
        wr32(base, VIRTIO_MMIO_STATUS, VIRTIO_STATUS_FAILED);
        return -1;
    }
    memzero(d->req, 4096);
    memzero(d->data, 4096);
    if (queue_init(d) != 0) {
        wr32(base, VIRTIO_MMIO_STATUS, VIRTIO_STATUS_FAILED);
        return -1;
    }

    d->capacity = *(volatile uint64_t *)(uintptr_t)(base + VIRTIO_MMIO_CONFIG);
    wr32(base, VIRTIO_MMIO_STATUS,
         VIRTIO_STATUS_ACK | VIRTIO_STATUS_DRIVER | VIRTIO_STATUS_FEATURES_OK |
             VIRTIO_STATUS_DRIVER_OK);
    k_log("virtio: mmio block ready\n");
    return 0;
}

/* --- request path --------------------------------------------------------- */

/* One request for up to 4096 bytes. TYPE: 0 = read, 1 = write. */
static int virtio_req(struct virtio_dev *d, uint32_t type, uint64_t lba,
                      void *buf, uint32_t bytes)
{
    uint16_t idx;

    *(uint32_t *)(d->req + 0) = type;
    *(uint32_t *)(d->req + 4) = 0;
    *(uint64_t *)(d->req + 8) = lba;
    d->req[16] = 0xff;
    if (type == VIRTIO_BLK_T_OUT) {
        uint8_t *src = buf;
        uint32_t i;
        for (i = 0; i < bytes; i++) {
            d->data[i] = src[i];
        }
    }

    d->desc[0].addr = d->req_phys;
    d->desc[0].len = 16;
    d->desc[0].flags = VRING_DESC_F_NEXT;
    d->desc[0].next = 1;
    d->desc[1].addr = d->data_phys;
    d->desc[1].len = bytes;
    d->desc[1].flags = ((type == VIRTIO_BLK_T_IN) ? VRING_DESC_F_WRITE : 0) |
                       VRING_DESC_F_NEXT;
    d->desc[1].next = 2;
    d->desc[2].addr = d->req_phys + 16;
    d->desc[2].len = 1;
    d->desc[2].flags = VRING_DESC_F_WRITE;
    d->desc[2].next = 0;

    idx = d->avail->idx;
    d->avail->ring[idx % QUEUE_SIZE] = 0;
    fence();
    d->avail->idx = idx + 1;
    fence();
    wr32(d->base, VIRTIO_MMIO_QUEUE_NOTIFY, 0);

    for (volatile uint64_t spins = 0; d->used->idx == idx; spins++) {
        if (spins > 500000000ull) {
            k_log("virtio: request timeout\n");
            return -1;
        }
    }
    fence();
    if (d->req[16] != 0) {
        k_log("virtio: request failed, status=");
        k_log_hex(d->req[16]);
        k_log("\n");
        return -1;
    }
    if (type == VIRTIO_BLK_T_IN) {
        uint8_t *dst = buf;
        uint32_t i;
        for (i = 0; i < bytes; i++) {
            dst[i] = d->data[i];
        }
    }
    return 0;
}

static int virtio_rw(void *priv, uint64_t lba, void *buf, size_t sectors,
                     uint32_t type)
{
    struct virtio_dev *d = priv;
    uint8_t *p = buf;

    if (d == NULL || d->base == 0 || buf == NULL || sectors == 0) {
        return -1;
    }
    while (sectors > 0) {
        uint32_t n = sectors > 8 ? 8 : (uint32_t)sectors;
        if (virtio_req(d, type, lba, p, n * 512) != 0) {
            return -1;
        }
        lba += n;
        p += n * 512;
        sectors -= n;
    }
    return 0;
}

static int virtio_read(void *priv, uint64_t lba, void *buf, size_t sectors)
{
    return virtio_rw(priv, lba, buf, sectors, VIRTIO_BLK_T_IN);
}

static int virtio_write(void *priv, uint64_t lba, const void *buf, size_t sectors)
{
    return virtio_rw(priv, lba, (void *)buf, sectors, VIRTIO_BLK_T_OUT);
}

static int virtio_identity(void *priv, struct blk_identity *out)
{
    struct virtio_dev *d = priv;
    static const char model[] = "virtio-mmio block";
    static const char serial[] = "virtio-mmio-0";
    int i;

    if (d == NULL || out == NULL) {
        return -1;
    }
    for (i = 0; i < BLK_MODEL_MAX; i++) {
        out->model[i] = i < (int)sizeof(model) ? model[i] : 0;
    }
    for (i = 0; i < BLK_SERIAL_MAX; i++) {
        out->serial[i] = i < (int)sizeof(serial) ? serial[i] : 0;
    }
    out->model[BLK_MODEL_MAX] = 0;
    out->serial[BLK_SERIAL_MAX] = 0;
    out->sectors = d->capacity;
    out->ssd = 1;
    return 0;
}

static const struct blk_ops virtio_ops = {
    .name = "virtio",
    .read = virtio_read,
    .write = virtio_write,
    .identity = virtio_identity,
    .smart_read_data = NULL,
    .smart_read_log = NULL,
};

/* Scan the virtio-mmio window at BASE (virtual); register the first block
 * device found. Returns 0 on success, -1 when no device is present. */
int virtio_mmio_init(uint64_t base)
{
    uint32_t i;

    for (i = 0; i < VIRTIO_MMIO_SLOTS; i++) {
        if (dev_probe(base + i * VIRTIO_MMIO_STRIDE) == 0) {
            return blk_register(&virtio_ops, &g_dev);
        }
    }
    return -1;
}
