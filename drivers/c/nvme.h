/* nvme.h — internal NVMe definitions shared by nvme.c (helper/core +
 * probe) and nvme_io.c (identity, SMART, data path). Not part of the
 * rust_core.h boundary. Reference: NVM Express 1.4. */
#ifndef FANTUAN_NVME_H
#define FANTUAN_NVME_H

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

/* The single controller this driver drives (defined in nvme.c). */
extern struct nvme_ctrl g_ctrl;

/* The ops table registered by nvme_probe (defined in nvme_io.c). */
extern const struct blk_ops NVME_OPS;

/* Shared core helpers (defined in nvme.c). */
int wait_csts(struct nvme_ctrl *c, uint32_t want, uint32_t tries);
void zero(void *p, size_t n);
void copy(void *dst, const void *src, size_t n);
int submit(struct nvme_ctrl *c, int admin, uint8_t opcode, uint32_t nsid,
           uint32_t cdw10, uint32_t cdw11, uint32_t cdw12, uint64_t prp1);

#endif /* FANTUAN_NVME_H */
