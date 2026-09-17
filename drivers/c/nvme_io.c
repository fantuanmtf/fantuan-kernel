/* nvme_io.c — NVMe identity decode, SMART/Health log, and the data path
 * behind the shared blk_ops table. Split out of nvme.c to keep every
 * file inside the size rule. */

#include "nvme.h"

/* --- identity + SMART ----------------------------------------------------- */

static int nvme_identity(void *priv, struct blk_identity *out)
{
    struct nvme_ctrl *c = (struct nvme_ctrl *)priv;
    uint8_t *id = c->buf;
    int i, n;

    if (c == NULL || !c->inited || out == NULL) {
        return -1;
    }
    /* Identify Controller (CNS=1): model at 24..63, serial at 4..23. */
    if (submit(c, 1, NVME_ADMIN_IDENTIFY, 0, NVME_CNS_CONTROLLER, 0, 0, c->buf_phys)) {
        return -1;
    }
    zero(out, sizeof(*out));
    for (i = 0, n = 0; i < 20 && n < BLK_MODEL_MAX; i++) {
        if (id[24 + i] == 0) {
            break;
        }
        out->model[n++] = (char)id[24 + i];
    }
    while (n > 0 && out->model[n - 1] == ' ') {
        out->model[--n] = 0;
    }
    for (i = 0, n = 0; i < 20 && n < BLK_SERIAL_MAX; i++) {
        if (id[4 + i] == 0) {
            break;
        }
        out->serial[n++] = (char)id[4 + i];
    }
    while (n > 0 && out->serial[n - 1] == ' ') {
        out->serial[--n] = 0;
    }
    out->ssd = 1;   /* NVMe namespaces are flash by definition */

    /* Identify Namespace 1 (CNS=0): NSZE (u64) = size in logical blocks. */
    if (submit(c, 1, NVME_ADMIN_IDENTIFY, 1, NVME_CNS_NAMESPACE, 0, 0, c->buf_phys)) {
        return -1;
    }
    out->sectors = (uint64_t)id[0] | ((uint64_t)id[1] << 8) |
                   ((uint64_t)id[2] << 16) | ((uint64_t)id[3] << 24) |
                   ((uint64_t)id[4] << 32) | ((uint64_t)id[5] << 40) |
                   ((uint64_t)id[6] << 48) | ((uint64_t)id[7] << 56);
    return 0;
}

/* Get Log Page (SMART/Health, page 0x02) into a caller buffer. */
static int nvme_smart_read_log(void *priv, uint8_t page, void *buf, size_t sectors)
{
    struct nvme_ctrl *c = (struct nvme_ctrl *)priv;
    uint32_t numdl;

    if (c == NULL || !c->inited || buf == NULL || sectors != 1) {
        return -1;
    }
    /* NUMDL counts DWORDS minus one: 512 bytes = 128 dwords -> 127. */
    numdl = (uint32_t)(sectors * 128 - 1) & 0xFFFu;
    zero(c->buf, 512);
    if (submit(c, 1, NVME_ADMIN_GET_LOG, 0xFFFFFFFFu,
               (uint32_t)page | (numdl << 16), 0, 0, c->buf_phys)) {
        return -1;
    }
    copy(buf, c->buf, sectors * 512);
    return 0;
}

/* ATA-style SMART page does not exist on NVMe. */
static int nvme_smart_read_data(void *priv, void *out_512)
{
    (void)priv;
    (void)out_512;
    return -1;
}

/* --- data path ------------------------------------------------------------ */

static int nvme_rw(struct nvme_ctrl *c, uint64_t lba, void *buf, size_t sectors,
                   int write)
{
    uint8_t *p = (uint8_t *)buf;

    if (c == NULL || !c->inited) {
        return -1;
    }
    while (sectors > 0) {
        size_t n = sectors > 8 ? 8 : sectors;
        if (write) {
            copy(c->buf, p, n * 512);
        }
        if (submit(c, 0, write ? NVME_IO_WRITE : NVME_IO_READ, 1,
                   (uint32_t)(lba & 0xFFFFFFFFu),
                   (uint32_t)(lba >> 32),
                   (uint32_t)(n - 1), c->buf_phys)) {
            return -1;
        }
        if (!write) {
            copy(p, c->buf, n * 512);
        }
        p += n * 512;
        lba += n;
        sectors -= n;
    }
    return 0;
}

static int nvme_read(void *priv, uint64_t lba, void *buf, size_t sectors)
{
    return nvme_rw((struct nvme_ctrl *)priv, lba, buf, sectors, 0);
}

static int nvme_write(void *priv, uint64_t lba, const void *buf, size_t sectors)
{
    return nvme_rw((struct nvme_ctrl *)priv, lba, (void *)buf, sectors, 1);
}

const struct blk_ops NVME_OPS = {
    .name = "nvme",
    .read = nvme_read,
    .write = nvme_write,
    .identity = nvme_identity,
    .smart_read_data = nvme_smart_read_data,
    .smart_read_log = nvme_smart_read_log,
};

