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

/* --- one-shot ATA command (shared issuer) --------------------------------- */
/* Issues a SECTOR_COUNT * 512-byte data-in (write=0) or data-out (write=1)
 * command on slot 0. FEATURES is placed in the Features register.
 * For writes the caller's data (in DST) is copied into the DMA buffer
 * before the command is issued.  SECTOR_COUNT must be <= 8 (fits in the
 * 4K single-page DMA buffer). */
static int ata_io_ex(uint8_t cmd, uint8_t device, uint64_t lba, uint8_t *dst,
                     int write, uint8_t features, uint16_t sector_count)
{
    struct ahci_port *p = &g_port;
    uint8_t *cfis = p->ct->cfis;
    uint32_t byte_count = (uint32_t)sector_count * 512u;
    int i;

    if (sector_count == 0 || sector_count > 8) {
        return -1;
    }

    if (write) {
        for (i = 0; i < (int)byte_count; i++) {
            p->buf[i] = dst[i];
        }
    }

    for (i = 0; i < 64; i++) {
        cfis[i] = 0;
    }
    cfis[0] = 0x27;  /* H2D register FIS */
    cfis[1] = 0x80;  /* C bit: this is a command */
    cfis[2] = cmd;
    cfis[3] = features;
    cfis[4] = (uint8_t)(lba & 0xFF);
    cfis[5] = (uint8_t)((lba >> 8) & 0xFF);
    cfis[6] = (uint8_t)((lba >> 16) & 0xFF);
    cfis[7] = device;
    /* LBA48 bits 24..47: without these, disks above 8 GiB read the wrong
     * sectors (the low 24 bits alias every 8 GiB). */
    cfis[8] = (uint8_t)((lba >> 24) & 0xFF);
    cfis[9] = (uint8_t)((lba >> 32) & 0xFF);
    cfis[10] = (uint8_t)((lba >> 40) & 0xFF);
    cfis[11] = 0;    /* Features (15:8) */
    cfis[12] = (uint8_t)(sector_count & 0xFF);
    cfis[13] = (uint8_t)((sector_count >> 8) & 0xFF);

    /* CFL (bits 4:0) = 5: the H2D register FIS is 5 DWORDs. Bit 6 = write.
     * PRDT presence is conveyed by prdtl, not by a flag. */
    p->clb[0].flags = 5 | (write ? (1u << 6) : 0);
    p->clb[0].prdtl = 1;
    p->clb[0].prdbc = 0;
    p->clb[0].ctba = (uint32_t)p->ct_phys;
    p->clb[0].ctbau = (uint32_t)(p->ct_phys >> 32);

    p->ct->prdt[0].dba = (uint32_t)p->buf_phys;
    p->ct->prdt[0].dbau = (uint32_t)(p->buf_phys >> 32);
    p->ct->prdt[0].dbc = byte_count - 1;   /* N*512 bytes, no IRQ on complete */

    p->px[PX_CI / 4] = 1;                   /* issue slot 0 */
    if (wait_until(&p->px[PX_CI / 4], 1, 0, 2000)) {
        k_log("ahci: command timeout");
        return -1;
    }
    if (p->px[PX_TFD / 4] & PX_TFD_ERR) {
        k_log("ahci: device error");
        return -1;
    }

    if (!write) {
        for (i = 0; i < (int)byte_count; i++) {
            dst[i] = p->buf[i];
        }
    }
    return 0;
}

/* Back-compat wrapper: 1 sector, features = 0. */
static int ata_io(uint8_t cmd, uint8_t device, uint64_t lba, uint8_t *dst,
                  int write)
{
    return ata_io_ex(cmd, device, lba, dst, write, 0, 1);
}

/* --- single-sector read (READ SECTORS EXT) -------------------------------- */
static int read_one(uint64_t lba, uint8_t *dst)
{
    return ata_io(0x24, 0x40, lba, dst, 0);    /* device: LBA mode */
}

/* --- identity decode (M5.5/M8: generic blk_identity) ---------------------- */

/* ATA IDENTIFY words are byte-swapped on the wire: word N lives at bytes
 * [2N+1, 2N] in the DMA buffer. */
static void ata_string(const uint8_t *id, int first_word, int words,
                       char *out, int out_max)
{
    int i, n = 0;
    for (i = 0; i < words && n < out_max - 1; i++) {
        char hi = (char)id[(first_word + i) * 2 + 1];
        char lo = (char)id[(first_word + i) * 2];
        /* Keep interior spaces; stop at a NUL and trim trailing spaces. */
        if (hi != 0 && n < out_max - 1) {
            out[n++] = hi;
        }
        if (lo != 0 && n < out_max - 1) {
            out[n++] = lo;
        }
    }
    while (n > 0 && out[n - 1] == ' ') {
        n--;
    }
    out[n] = 0;
}

static int ahci_identity(void *priv, struct blk_identity *out)
{
    struct ahci_port *p = (struct ahci_port *)priv;
    uint8_t id[512];
    uint64_t cap = 0;
    int i;

    if (p == NULL || !p->inited || out == NULL) {
        return -1;
    }
    if (ata_io(0xEC, 0xA0, 0, id, 0)) {
        return -1;
    }
    ata_string(id, 27, 20, out->model, BLK_MODEL_MAX + 1);
    ata_string(id, 10, 10, out->serial, BLK_SERIAL_MAX + 1);
    for (i = 0; i < 4; i++) {
        cap |= (uint64_t)((uint16_t)id[(100 + i) * 2] |
                          ((uint16_t)id[(100 + i) * 2 + 1] << 8)) << (16 * i);
    }
    out->sectors = cap;
    /* Word 217: 1 = non-rotating (SSD). */
    out->ssd = (((uint16_t)id[217 * 2] | ((uint16_t)id[217 * 2 + 1] << 8)) == 1);
    return 0;
}

/* --- ops table (registered with blk.c) ------------------------------------ */

static int ahci_read(void *priv, uint64_t lba, void *buf, size_t sectors)
{
    struct ahci_port *p = (struct ahci_port *)priv;
    size_t s;
    if (p == NULL || !p->inited) {
        return -1;
    }
    for (s = 0; s < sectors; s++) {
        if (read_one(lba + s, (uint8_t *)buf + s * 512)) {
            return -1;
        }
    }
    return 0;
}

static int ahci_write(void *priv, uint64_t lba, const void *buf, size_t sectors)
{
    struct ahci_port *p = (struct ahci_port *)priv;
    size_t s;
    if (p == NULL || !p->inited) {
        return -1;
    }
    for (s = 0; s < sectors; s++) {
        if (ata_io(0x35, 0x40, lba + s, (uint8_t *)buf + s * 512, 1)) {
            return -1;
        }
    }
    return 0;
}

/* SMART READ DATA: 512-byte attribute page. The ATA signature lives in the
 * LBA registers (mid = 0x4F, high = 0xC2) with features = 0xD0. */
static int ahci_smart_read_data(void *priv, void *out_512)
{
    struct ahci_port *p = (struct ahci_port *)priv;
    uint64_t sig;
    if (p == NULL || !p->inited || out_512 == NULL) {
        return -1;
    }
    sig = ((uint64_t)0xC2 << 16) | ((uint64_t)0x4F << 8);
    return ata_io_ex(0xB0, 0x40, sig, (uint8_t *)out_512, 0, 0xD0, 1);
}

/* SMART READ LOG for LOG_PAGE: the page number goes in LBA low. */
static int ahci_smart_read_log(void *priv, uint8_t log_page, void *buf, size_t sectors)
{
    struct ahci_port *p = (struct ahci_port *)priv;
    uint64_t sig;
    if (p == NULL || !p->inited || buf == NULL || sectors == 0 || sectors > 8) {
        return -1;
    }
    sig = ((uint64_t)0xC2 << 16) | ((uint64_t)0x4F << 8) | (uint64_t)log_page;
    return ata_io_ex(0xB0, 0x40, sig, (uint8_t *)buf, 0, 0xD5, (uint16_t)sectors);
}

static const struct blk_ops AHCI_OPS = {
    .name = "ahci",
    .read = ahci_read,
    .write = ahci_write,
    .identity = ahci_identity,
    .smart_read_data = ahci_smart_read_data,
    .smart_read_log = ahci_smart_read_log,
};

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
            if (init_port(port) != 0) {
                return -1;
            }
            if (blk_register(&AHCI_OPS, &g_port) < 0) {
                k_log("ahci: device table full\n");
                return -1;
            }
            return 0;
        }
    }
    k_log("ahci: no implemented ports");
    return -1;
}
