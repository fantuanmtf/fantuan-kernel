/* ahci.c — AHCI (SATA) probe and port bring-up, v1 (M4.5).
 * Polling only: no interrupts, no writes, one command slot.
 * The command path lives in ahci_io.c; both include ahci.h.
 * See docs/DESIGN.md §5 (C driver layer) and the AHCI 1.3 spec. */

#include "ahci.h"

static volatile uint32_t *g_abar;
struct ahci_port g_ports[AHCI_MAX_PORTS];

/* --- helpers ------------------------------------------------------------- */
int wait_until(volatile uint32_t *reg, uint32_t mask, uint32_t want,
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
static int init_port(struct ahci_port *p, int port)
{
    uint64_t phys;

    p->px = &g_abar[PORT_BASE(port) / 4];

    /* stop the port and wait for the command engine to drain */
    p->px[PX_CMD / 4] &= ~PX_CMD_ST;
    if (wait_until(&p->px[PX_CMD / 4], PX_CMD_CR, 0, 2000)) {
        k_log("ahci: timeout stopping port");
        return -1;
    }

    /* allocate the command list, command table, DMA buffer and FIS area */
    p->clb = (struct cmd_header *)k_alloc_page(&phys);
    p->clb_phys = phys;
    p->ct = (struct cmd_table *)k_alloc_page(&phys);
    p->ct_phys = phys;
    p->buf = (uint8_t *)k_alloc_page(&phys);
    p->buf_phys = phys;
    p->fis = (uint8_t *)k_alloc_page(&phys);
    p->fis_phys = phys;

    /* install and start: FIS receive needs a valid receive area BEFORE FRE
     * is set (the HBA DMAs FISes there; a null base would write to phys 0) */
    p->px[PX_CLB / 4] = (uint32_t)p->clb_phys;
    p->px[PX_CLB / 4 + 1] = (uint32_t)(p->clb_phys >> 32);
    p->px[PX_FB / 4] = (uint32_t)p->fis_phys;
    p->px[PX_FB / 4 + 1] = (uint32_t)(p->fis_phys >> 32);
    p->px[PX_IE / 4] = 0; /* no interrupts in v1: pure polling */
    p->px[PX_CMD / 4] |= PX_CMD_FRE;
    p->px[PX_CMD / 4] |= PX_CMD_ST;
    if (wait_until(&p->px[PX_CMD / 4], PX_CMD_CR, PX_CMD_CR, 2000)) {
        k_log("ahci: port failed to start");
        return -1;
    }

    p->inited = 1;
    p->tag = AHCI_PORT_TAG;
    k_log("ahci: port ");
    k_log_hex((uint64_t)port);
    k_log(" started\n");
    return 0;
}


/* --- exported probe -------------------------------------------------------- */
/* Called from the Rust core with the ABAR (physical) found by PCI scan.
 * Registers every implemented port that reports a device (PX_SSTS.DET == 3),
 * in port order, so blk_open(0..n) addresses the attached disks. */
int ahci_probe(uint64_t abar_phys)
{
    uint32_t pi;
    int port, registered = 0;

    g_abar = (volatile uint32_t *)k_phys_to_virt(abar_phys);
    g_abar[HBA_GHC / 4] |= GHC_AE;

    pi = g_abar[HBA_PI / 4];
    k_log("ahci: implemented ports = ");
    k_log_hex(pi);
    k_log("\n");

    for (port = 0; port < 32; port++) {
        volatile uint32_t *px;
        if (!(pi & (1u << port))) {
            continue;
        }
        if (registered >= AHCI_MAX_PORTS) {
            k_log("ahci: port table full\n");
            break;
        }
        px = &g_abar[PORT_BASE(port) / 4];
        if ((px[PX_SSTS / 4] & 0x0Fu) != 3) {
            k_log("ahci: port ");
            k_log_hex((uint64_t)port);
            k_log(" empty\n");
            continue;
        }
        if (init_port(&g_ports[registered], port) != 0) {
            k_log("ahci: port ");
            k_log_hex((uint64_t)port);
            k_log(" unusable\n");
            continue;
        }
        if (blk_register(&AHCI_OPS, &g_ports[registered]) < 0) {
            k_log("ahci: device table full\n");
            break;
        }
        registered++;
    }
    if (registered == 0) {
        k_log("ahci: no device on any implemented port");
        return -1;
    }
    return 0;
}
