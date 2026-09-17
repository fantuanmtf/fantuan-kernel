/* ahci.h — internal AHCI definitions shared by ahci.c (probe/port
 * bring-up) and ahci_io.c (command path). Not part of the rust_core.h
 * boundary; the C layer never exposes these. */
#ifndef FANTUAN_AHCI_H
#define FANTUAN_AHCI_H

#include <stdint.h>
#include <stddef.h>
#include "rust_core.h"
#include "driver.h"

/* --- HBA register offsets (memory-mapped, relative to ABAR) ------------ */
#define HBA_CAP      0x00u
#define HBA_GHC      0x04u
#define HBA_PI       0x0Cu
#define PORT_BASE(p) (0x100u + (p) * 0x80u)

#define PX_CLB  0x00u
#define PX_FB   0x08u
#define PX_IS   0x10u
#define PX_IE   0x14u
#define PX_CMD  0x18u
#define PX_TFD  0x20u
#define PX_SIG  0x24u
#define PX_SSTS 0x28u
#define PX_SCTL 0x2Cu
#define PX_SERR 0x30u
#define PX_SACT 0x34u
#define PX_CI   0x38u

/* --- bit definitions ---------------------------------------------------- */
#define GHC_AE (1u << 31)      /* AHCI enable */
#define PX_CMD_ST  (1u << 0)   /* start */
#define PX_CMD_FRE (1u << 4)   /* FIS receive enable */
#define PX_CMD_FR  (1u << 14)  /* FIS receive running */
#define PX_CMD_CR  (1u << 15)  /* command list running */
#define PX_TFD_ERR (1u << 0)   /* error bit */

/* --- command structures (AHCI 1.3) -------------------------------------- */
struct cmd_header {
    uint16_t flags;     /* bit5: PRDT present */
    uint16_t prdtl;
    uint32_t prdbc;
    uint32_t ctba;      /* command table physical address */
    uint32_t ctbau;
    uint32_t rsv[4];
};

struct cmd_table {
    uint8_t cfis[64];
    uint8_t acmd[16];
    uint8_t rsv[48];
    struct {
        uint32_t dba;   /* data base address (physical) */
        uint32_t dbau;
        uint32_t rsv;
        uint32_t dbc;   /* byte count - 1; bit31 = interrupt on complete */
    } prdt[1];
};

#define AHCI_PORT_TAG 0x41484349u   /* "AHCI" */

/* --- per-port driver state ---------------------------------------------- */
struct ahci_port {
    uint32_t tag;            /* magic: AHCI_PORT_TAG for validation */
    volatile uint32_t *px;   /* port register block */
    struct cmd_header *clb;
    uint64_t clb_phys;
    struct cmd_table *ct;
    uint64_t ct_phys;
    uint8_t *buf;            /* one-page (4K) DMA buffer */
    uint64_t buf_phys;
    uint8_t *fis;            /* one-page FIS receive area (PX_FB) */
    uint64_t fis_phys;
    int inited;
};


/* The single controller this driver drives (defined in ahci.c). */
extern struct ahci_port g_port;

/* The ops table registered by ahci_probe (defined in ahci_io.c). */
extern const struct blk_ops AHCI_OPS;

/* Shared polling helper (defined in ahci.c). */
int wait_until(volatile uint32_t *reg, uint32_t mask, uint32_t want, uint32_t tries);

#endif /* FANTUAN_AHCI_H */
