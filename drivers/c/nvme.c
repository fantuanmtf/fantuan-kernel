/* nvme.c — NVMe (PCIe) storage driver, v1 (DESIGN.md §5, "NVMe second").
 *
 * Polling only, one admin queue and one I/O queue pair, one 4 KiB DMA page
 * per transfer (max 8 sectors). Registers the same blk_ops table as AHCI, so
 * the kernel above never learns which transport served the bytes.
 *
 * Reference: NVM Express 1.4 (controller registers, admin commands, PRP).
 */

#include <stdint.h>
#include <stddef.h>
#include "rust_core.h"
#include "driver.h"

/* --- controller registers (BAR0, 32-bit MMIO) ----------------------------- */
#define NVME_CAP    0x00u
#define NVME_VS     0x08u
#define NVME_INTMS  0x0Cu
#define NVME_INTMC  0x10u
#define NVME_CC     0x14u
#define NVME_CSTS   0x1Cu
#define NVME_AQA    0x24u
#define NVME_ASQ    0x28u
#define NVME_ACQ    0x30u

#define NVME_CC_EN       (1u << 0)
#define NVME_CSTS_RDY    (1u << 0)
#define NVME_CC_IOSQES   (6u << 16)   /* 64-byte submission entries */
#define NVME_CC_IOCQES   (4u << 20)   /* 16-byte completion entries */
/* Doorbell base; the stride between queues is (8 << CAP.DSTRD) bytes, with
 * the CQ head doorbell one (4 << DSTRD) further on (NVMe 1.4 §3.1.4). */
#define NVME_DBS_BASE    0x1000u

/* --- admin opcodes -------------------------------------------------------- */
#define NVME_ADMIN_CREATE_SQ 0x01u
#define NVME_ADMIN_GET_LOG   0x02u
#define NVME_ADMIN_CREATE_CQ 0x05u
#define NVME_ADMIN_IDENTIFY  0x06u
#define NVME_IO_WRITE        0x01u
#define NVME_IO_READ         0x02u

#define NVME_LOG_SMART       0x02u
#define NVME_CNS_NAMESPACE   0x00u
#define NVME_CNS_CONTROLLER  0x01u

#define NVME_TAG 0x4E564D45u          /* "NVME" */
/* Desired queue depth; the controller's CAP.MQES caps it (see nvme_probe).
 * One 4 KiB page holds both SQ and CQ for depths up to 64. */
#define NVME_Q_ENTRIES 4u
/* Command completion poll budget, in milliseconds. NVMe commands complete in
 * microseconds; 5 s is already a "controller is wedged" timeout. */
#define NVME_CMD_TIMEOUT_MS 5000u

struct nvme_cmd {                     /* 64-byte submission entry */
    uint32_t cdw0;                    /* opcode | (cid << 16) */
    uint32_t nsid;
    uint64_t rsvd;
    uint64_t mptr;
    uint64_t prp1;
    uint64_t prp2;
    uint32_t cdw10;
    uint32_t cdw11;
    uint32_t cdw12;
    uint32_t cdw13;
    uint32_t cdw14;
    uint32_t cdw15;
};

struct nvme_cqe {                     /* 16-byte completion entry */
    uint32_t dw0;
    uint32_t dw1;
    uint16_t sq_head;
    uint16_t sq_id;
    uint16_t cid;
    uint16_t status;                  /* bit 0 = phase tag */
};

struct nvme_ctrl {
    uint32_t tag;
    volatile uint32_t *regs;
    uint8_t *asq;
    uint64_t asq_phys;
    uint8_t *acq;
    uint64_t acq_phys;
    uint8_t *iosq;
    uint64_t iosq_phys;
    uint8_t *iocq;
    uint64_t iocq_phys;
    uint8_t *buf;                     /* one 4 KiB data page */
    uint64_t buf_phys;
    uint32_t dstrd;                   /* doorbell stride (CAP bits 35:32) */
    uint32_t q_entries;               /* actual queue depth (≤ NVME_Q_ENTRIES) */
    uint32_t nsze_lo, nsze_hi;        /* namespace size in LBAs */
    uint32_t adm_tail, adm_phase, adm_cq_head;
    uint32_t io_tail, io_phase, io_cq_head;
    uint16_t cid;
    int inited;
};

static struct nvme_ctrl g_ctrl;

/* --- helpers -------------------------------------------------------------- */

static int wait_csts(struct nvme_ctrl *c, uint32_t want, uint32_t tries)
{
    while (tries--) {
        if ((c->regs[NVME_CSTS / 4] & NVME_CSTS_RDY) == want) {
            return 0;
        }
        k_delay_ms(1);
    }
    return -1;
}

static void zero(void *p, size_t n)
{
    size_t i;
    uint8_t *b = (uint8_t *)p;
    for (i = 0; i < n; i++) {
        b[i] = 0;
    }
}

static void copy(void *dst, const void *src, size_t n)
{
    size_t i;
    uint8_t *d = (uint8_t *)dst;
    const uint8_t *s = (const uint8_t *)src;
    for (i = 0; i < n; i++) {
        d[i] = s[i];
    }
}

/* Submit one command into Q and poll its completion. Returns 0 when the
 * completion carries no error status (bits 1..15 of the status word). */
static int submit(struct nvme_ctrl *c, int admin, uint8_t opcode, uint32_t nsid,
                  uint32_t cdw10, uint32_t cdw11, uint32_t cdw12,
                  uint64_t prp1)
{
    struct nvme_cmd *sq = (struct nvme_cmd *)(admin ? c->asq : c->iosq);
    struct nvme_cqe *cq = (struct nvme_cqe *)(admin ? c->acq : c->iocq);
    uint32_t *tail = admin ? &c->adm_tail : &c->io_tail;
    uint32_t *phase = admin ? &c->adm_phase : &c->io_phase;
    uint32_t *head = admin ? &c->adm_cq_head : &c->io_cq_head;
    uint32_t qid = admin ? 0u : 1u;
    uint32_t entries = c->q_entries;
    struct nvme_cmd *cmd = &sq[*tail];
    uint32_t cqe_idx;
    uint32_t tries = NVME_CMD_TIMEOUT_MS;
    uint16_t status;

    zero(cmd, sizeof(*cmd));
    cmd->cdw0 = (uint32_t)opcode | ((uint32_t)(c->cid++) << 16);
    cmd->nsid = nsid;
    cmd->prp1 = prp1;
    cmd->cdw10 = cdw10;
    cmd->cdw11 = cdw11;
    cmd->cdw12 = cdw12;

    __sync_synchronize();
    *tail = (*tail + 1) % entries;
    /* Doorbells: SQyTDBL at 0x1000 + 2y*(4<<DSTRD), CQyHDBL one stride on. */
    c->regs[(NVME_DBS_BASE + qid * (8u << c->dstrd)) / 4] = *tail;

    cqe_idx = *head;
    for (;;) {
        status = cq[cqe_idx].status;
        if ((status & 1u) == *phase) {
            break;
        }
        if (tries-- == 0) {
            return -1;
        }
        k_delay_ms(1);
    }
    *head = (cqe_idx + 1) % entries;
    c->regs[(NVME_DBS_BASE + qid * (8u << c->dstrd) + (4u << c->dstrd)) / 4] = *head;
    if (*head == 0) {
        *phase ^= 1u;
    }
    return (status >> 1) == 0 ? 0 : -1;
}

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

static const struct blk_ops NVME_OPS = {
    .name = "nvme",
    .read = nvme_read,
    .write = nvme_write,
    .identity = nvme_identity,
    .smart_read_data = nvme_smart_read_data,
    .smart_read_log = nvme_smart_read_log,
};

/* --- probe ---------------------------------------------------------------- */

/* Called from the Rust core with BAR0 (physical) from the PCI catalog. */
int nvme_probe(uint64_t bar0_phys)
{
    struct nvme_ctrl *c = &g_ctrl;
    uint64_t cap;
    uint64_t phys;

    c->regs = (volatile uint32_t *)k_phys_to_virt(bar0_phys);
    cap = (uint64_t)c->regs[NVME_CAP / 4] | ((uint64_t)c->regs[NVME_CAP / 4 + 1] << 32);
    c->dstrd = (uint32_t)((cap >> 32) & 0xF);
    /* The Rust core maps a 64 KiB register window; a larger stride would put
     * the doorbells outside it. Real controllers use 0..4. */
    if (c->dstrd > 8u) {
        k_log("nvme: doorbell stride above the mapped window — unsupported\n");
        return -1;
    }
    /* CAP.MQES is zero-based: a controller may support fewer entries than the
     * driver would like, and programming AQA/CQ/SQ beyond MQES is undefined. */
    {
        uint32_t mqes = (uint32_t)(cap & 0xFFFFu) + 1u;
        c->q_entries = mqes < NVME_Q_ENTRIES ? mqes : NVME_Q_ENTRIES;
    }
    if (c->q_entries < 2u) {
        k_log("nvme: controller queue depth below 2 — unusable\n");
        return -1;
    }
    k_log("nvme: version ");
    k_log_hex(c->regs[NVME_VS / 4]);
    k_log(" mqes ");
    k_log_hex(cap & 0xFFFFu);
    k_log("\n");

    /* Reset: interrupts off, disable, wait for RDY clear. */
    c->regs[NVME_INTMS / 4] = 0xFFFFFFFFu;
    c->regs[NVME_CC / 4] = 0;
    if (wait_csts(c, 0, 2000)) {
        k_log("nvme: controller did not reset\n");
        return -1;
    }

    c->asq = (uint8_t *)k_alloc_page(&phys);
    c->asq_phys = phys;
    c->acq = (uint8_t *)k_alloc_page(&phys);
    c->acq_phys = phys;
    c->iosq = (uint8_t *)k_alloc_page(&phys);
    c->iosq_phys = phys;
    c->iocq = (uint8_t *)k_alloc_page(&phys);
    c->iocq_phys = phys;
    c->buf = (uint8_t *)k_alloc_page(&phys);
    c->buf_phys = phys;
    zero(c->asq, 4096);
    zero(c->acq, 4096);
    zero(c->iosq, 4096);
    zero(c->iocq, 4096);

    c->regs[NVME_AQA / 4] = ((c->q_entries - 1) << 16) | (c->q_entries - 1);
    c->regs[NVME_ASQ / 4] = (uint32_t)c->asq_phys;
    c->regs[NVME_ASQ / 4 + 1] = (uint32_t)(c->asq_phys >> 32);
    c->regs[NVME_ACQ / 4] = (uint32_t)c->acq_phys;
    c->regs[NVME_ACQ / 4 + 1] = (uint32_t)(c->acq_phys >> 32);

    c->adm_tail = 0;
    c->adm_phase = 1;
    c->adm_cq_head = 0;
    c->io_tail = 0;
    c->io_phase = 1;
    c->io_cq_head = 0;
    c->cid = 1;

    c->regs[NVME_CC / 4] = NVME_CC_EN | NVME_CC_IOSQES | NVME_CC_IOCQES;
    if (wait_csts(c, NVME_CSTS_RDY, 2000)) {
        k_log("nvme: controller did not become ready\n");
        return -1;
    }
    c->inited = 1;

    /* One I/O completion queue + submission queue (polling, no interrupts). */
    if (submit(c, 1, NVME_ADMIN_CREATE_CQ, 0,
               (c->q_entries - 1) << 16 | 1u, (1u << 0) /* PC */, 0, c->iocq_phys)) {
        k_log("nvme: create CQ failed\n");
        return -1;
    }
    if (submit(c, 1, NVME_ADMIN_CREATE_SQ, 0,
               (c->q_entries - 1) << 16 | 1u, (1u << 0) /* PC */ | (1u << 16) /* CQID=1 */,
               0, c->iosq_phys)) {
        k_log("nvme: create SQ failed\n");
        return -1;
    }

    c->tag = NVME_TAG;
    if (blk_register(&NVME_OPS, c) < 0) {
        k_log("nvme: device table full\n");
        return -1;
    }
    k_log("nvme: controller up (namespace 1)\n");
    return 0;
}
