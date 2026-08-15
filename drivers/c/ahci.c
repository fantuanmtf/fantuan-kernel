/* ahci.c — AHCI (SATA) read-only driver, v1 (M4.5).
 * Polling only: no interrupts, no writes, one command slot. Reads sectors
 * through a single 4K DMA buffer with a one-entry PRDT.
 * See docs/DESIGN.md §5 (C driver layer) and the AHCI 1.3 spec.
 */

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

/* --- per-port driver state ---------------------------------------------- */
struct ahci_port {
    volatile uint32_t *px;   /* port register block */
    struct cmd_header *clb;
    uint64_t clb_phys;
    struct cmd_table *ct;
    uint64_t ct_phys;
    uint8_t *buf;            /* one-sector DMA buffer */
    uint64_t buf_phys;
    int inited;
};

static volatile uint32_t *g_abar;
static struct ahci_port g_port;

/* --- helpers ------------------------------------------------------------- */
static int wait_until(volatile uint32_t *reg, uint32_t mask, uint32_t want,
                      uint32_t tries)
{
    while (tries--) {
        if (((*reg) & mask) == want) {
            return 0;
        }
        k_delay_ms(1);
    }
    return -1;
}

/* --- port bring-up -------------------------------------------------------- */
static int init_port(int port)
{
    struct ahci_port *p = &g_port;
    uint64_t phys;

    p->px = &g_abar[PORT_BASE(port) / 4];

    /* stop the port and wait for the command engine to drain */
    p->px[PX_CMD / 4] &= ~PX_CMD_ST;
    if (wait_until(&p->px[PX_CMD / 4], PX_CMD_CR, 0, 2000)) {
        k_log("ahci: timeout stopping port");
        return -1;
    }

    /* allocate the command list, command table and DMA buffer */
    p->clb = (struct cmd_header *)k_alloc_page(&phys);
    p->clb_phys = phys;
    p->ct = (struct cmd_table *)k_alloc_page(&phys);
    p->ct_phys = phys;
    p->buf = (uint8_t *)k_alloc_page(&phys);
    p->buf_phys = phys;

    /* install and start: FIS receive first, then the command engine */
    p->px[PX_CLB / 4] = (uint32_t)p->clb_phys;
    p->px[PX_CLB / 4 + 1] = (uint32_t)(p->clb_phys >> 32);
    p->px[PX_FB / 4] = 0;
    p->px[PX_FB / 4 + 1] = 0;
    p->px[PX_IE / 4] = 0; /* no interrupts in v1: pure polling */
    p->px[PX_CMD / 4] |= PX_CMD_FRE;
    p->px[PX_CMD / 4] |= PX_CMD_ST;
    if (wait_until(&p->px[PX_CMD / 4], PX_CMD_CR, PX_CMD_CR, 2000)) {
        k_log("ahci: port failed to start");
        return -1;
    }

    p->inited = 1;
    k_log("ahci: port ");
    k_log_hex((uint64_t)port);
    k_log(" started\n");
    return 0;
}

/* --- one-shot ATA command (shared issuer) --------------------------------- */
/* Issues a 512-byte data-in command on slot 0 and copies the result out. */
static int ata_io(uint8_t cmd, uint8_t device, uint64_t lba, uint8_t *dst)
{
    struct ahci_port *p = &g_port;
    uint8_t *cfis = p->ct->cfis;
    int i;

    for (i = 0; i < 64; i++) {
        cfis[i] = 0;
    }
    cfis[0] = 0x27;  /* H2D register FIS */
    cfis[1] = 0x80;  /* C bit: this is a command */
    cfis[2] = cmd;
    cfis[4] = (uint8_t)(lba & 0xFF);
    cfis[5] = (uint8_t)((lba >> 8) & 0xFF);
    cfis[6] = (uint8_t)((lba >> 16) & 0xFF);
    cfis[7] = device;
    cfis[12] = 1;    /* sector count = 1 */

    /* CFL (bits 4:0) = 5: the H2D register FIS is 5 DWORDs. PRDT presence
     * is conveyed by prdtl, not by a flag. */
    p->clb[0].flags = 5;
    p->clb[0].prdtl = 1;
    p->clb[0].prdbc = 0;
    p->clb[0].ctba = (uint32_t)p->ct_phys;
    p->clb[0].ctbau = (uint32_t)(p->ct_phys >> 32);

    p->ct->prdt[0].dba = (uint32_t)p->buf_phys;
    p->ct->prdt[0].dbau = (uint32_t)(p->buf_phys >> 32);
    p->ct->prdt[0].dbc = 511;               /* 512 bytes, no IRQ on complete */

    p->px[PX_CI / 4] = 1;                   /* issue slot 0 */
    if (wait_until(&p->px[PX_CI / 4], 1, 0, 2000)) {
        k_log("ahci: command timeout");
        return -1;
    }
    if (p->px[PX_TFD / 4] & PX_TFD_ERR) {
        k_log("ahci: device error");
        return -1;
    }

    for (i = 0; i < 512; i++) {
        dst[i] = p->buf[i];
    }
    return 0;
}

/* --- single-sector read (READ SECTORS EXT) -------------------------------- */
static int read_one(uint64_t lba, uint8_t *dst)
{
    return ata_io(0x24, 0x40, lba, dst);    /* device: LBA mode */
}

/* --- IDENTIFY DEVICE -------------------------------------------------------- */
int ata_identify(uint8_t *dst)
{
    return ata_io(0xEC, 0xA0, 0, dst);      /* device: LBA mode, master */
}

/* --- exported probe -------------------------------------------------------- */
/* Called from the Rust core with the ABAR (physical) found by PCI scan. */
int ahci_probe(uint64_t abar_phys)
{
    uint32_t pi;
    int port;

    g_abar = (volatile uint32_t *)k_phys_to_virt(abar_phys);
    g_abar[HBA_GHC / 4] |= GHC_AE;

    pi = g_abar[HBA_PI / 4];
    k_log("ahci: implemented ports = ");
    k_log_hex(pi);
    k_log("\n");

    for (port = 0; port < 32; port++) {
        if (pi & (1u << port)) {
            return init_port(port);
        }
    }
    k_log("ahci: no implemented ports");
    return -1;
}

/* --- block read ops -------------------------------------------------------- */
int blk_read(void *dev, uint64_t lba, void *buf, size_t sectors)
{
    size_t s;
    (void)dev;
    if (!g_port.inited) {
        return -1;
    }
    for (s = 0; s < sectors; s++) {
        if (read_one(lba + s, (uint8_t *)buf + s * 512)) {
            return -1;
        }
    }
    return 0;
}
