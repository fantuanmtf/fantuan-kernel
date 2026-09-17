/* nvme.c — NVMe (PCIe) core: polling helpers, the one-shot command
 * submitter, and controller bring-up (reset, queues, register). The
 * identity/SMART/data path live in nvme_io.c; both include nvme.h.
 * Registers the same blk_ops table as AHCI, so the kernel above never
 * learns which transport served the bytes. */

#include "nvme.h"

/* The single controller state lives here; nvme_io.c reaches it via nvme.h. */
struct nvme_ctrl g_ctrl;

/* --- helpers -------------------------------------------------------------- */

int wait_csts(struct nvme_ctrl *c, uint32_t want, uint32_t tries)
{
    while (tries--) {
        if ((c->regs[NVME_CSTS / 4] & NVME_CSTS_RDY) == want) {
            return 0;
        }
        k_delay_ms(1);
    }
    return -1;
}

void zero(void *p, size_t n)
{
    size_t i;
    uint8_t *b = (uint8_t *)p;
    for (i = 0; i < n; i++) {
        b[i] = 0;
    }
}

void copy(void *dst, const void *src, size_t n)
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
int submit(struct nvme_ctrl *c, int admin, uint8_t opcode, uint32_t nsid,
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
