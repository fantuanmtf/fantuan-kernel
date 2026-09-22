/* ahci_io.c — AHCI command path: one-shot ATA commands, sector reads and
 * writes, IDENTIFY decode and SMART, plus the blk_ops registration table.
 * Split out of ahci.c to keep every file inside the size rule. */

#include "ahci.h"

/* --- port error recovery --------------------------------------------------- */
/* A failed command leaves PxCI set (QEMU keeps the slot busy until software
 * clears it; real HBAs can also leave it set on a timeout), so the next
 * command would never run and every retry would burn the full timeout.
 * Stop the command engine (which clears PxCI), clear the latched IRQ/SERR
 * status (RW1C), then restart. The 1 MiB copy path relies on this to retry
 * and to continue past an unreadable sector. */
static void ahci_kick(struct ahci_port *p)
{
    uint32_t cmd = p->px[PX_CMD / 4];

    p->px[PX_CMD / 4] = cmd & ~PX_CMD_ST;
    (void)wait_until(&p->px[PX_CMD / 4], PX_CMD_CR, 0, 2000);
    p->px[PX_IS / 4] = 0xFFFFFFFFu;    /* write-1-to-clear */
    p->px[PX_SERR / 4] = 0xFFFFFFFFu;  /* write-1-to-clear */
    p->px[PX_CMD / 4] = cmd | PX_CMD_ST;
    (void)wait_until(&p->px[PX_CMD / 4], PX_CMD_CR, PX_CMD_CR, 2000);
}

/* --- one-shot ATA command (shared issuer) --------------------------------- */
/* Issues a SECTOR_COUNT * 512-byte data-in (write=0) or data-out (write=1)
 * command on slot 0. FEATURES is placed in the Features register.
 * For writes the caller's data (in DST) is copied into the DMA buffer
 * before the command is issued.  SECTOR_COUNT must be <= 8 (fits in the
 * 4K single-page DMA buffer). */
static int ata_io_ex(struct ahci_port *p, uint8_t cmd, uint8_t device,
                     uint64_t lba, uint8_t *dst, int write, uint8_t features,
                     uint16_t sector_count)
{
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
    /* Poll for either completion (CI clears) or a device error: on error CI
     * stays set (the HBA waits for software) while PxIS.TFES latches, which
     * the previous kick cleared, so waiting on CI alone would burn the full
     * 2 s timeout on every errored command. */
    for (i = 0; i < 2000; i++) {
        if (!(p->px[PX_CI / 4] & 1) || (p->px[PX_IS / 4] & PX_IS_TFES)) {
            break;
        }
        k_delay_ms(1);
    }
    if (p->px[PX_IS / 4] & PX_IS_TFES) {
        k_log("ahci: device error");
        ahci_kick(p);
        return -1;
    }
    if (p->px[PX_CI / 4] & 1) {
        k_log("ahci: command timeout");
        ahci_kick(p);
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
static int ata_io(struct ahci_port *p, uint8_t cmd, uint8_t device, uint64_t lba,
                  uint8_t *dst, int write)
{
    return ata_io_ex(p, cmd, device, lba, dst, write, 0, 1);
}

/* --- single-sector read (READ SECTORS EXT) -------------------------------- */
static int read_one(struct ahci_port *p, uint64_t lba, uint8_t *dst)
{
    return ata_io(p, 0x24, 0x40, lba, dst, 0);    /* device: LBA mode */
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
    if (ata_io(p, 0xEC, 0xA0, 0, id, 0)) {
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
        if (read_one(p, lba + s, (uint8_t *)buf + s * 512)) {
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
        if (ata_io(p, 0x35, 0x40, lba + s, (uint8_t *)buf + s * 512, 1)) {
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
    return ata_io_ex(p, 0xB0, 0x40, sig, (uint8_t *)out_512, 0, 0xD0, 1);
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
    return ata_io_ex(p, 0xB0, 0x40, sig, (uint8_t *)buf, 0, 0xD5, (uint16_t)sectors);
}

const struct blk_ops AHCI_OPS = {
    .name = "ahci",
    .read = ahci_read,
    .write = ahci_write,
    .identity = ahci_identity,
    .smart_read_data = ahci_smart_read_data,
    .smart_read_log = ahci_smart_read_log,
};

