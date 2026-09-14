/* driver.h — the C driver ops interface (DESIGN.md §5).
 *
 * Storage drivers implement the blk_ops table and register it once probed;
 * the Rust core only ever holds an opaque handle from blk_open() and calls
 * the driver-agnostic ops below. AHCI is the reference implementation; NVMe
 * registers the same table (the whole point of the abstraction).
 *
 * Ownership: the Rust core allocates DMA pages through k_alloc_page and keeps
 * them alive; drivers must not free them. Buffers passed to the ops are
 * caller-owned (kernel memory).
 */
#ifndef FANTUAN_DRIVER_H
#define FANTUAN_DRIVER_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* --- generic identity (decoded by the driver, formatted by the kernel) ---- */
#define BLK_MODEL_MAX  40
#define BLK_SERIAL_MAX 20

struct blk_identity {
    char     model[BLK_MODEL_MAX + 1];   /* NUL-terminated, trimmed */
    char     serial[BLK_SERIAL_MAX + 1];
    uint64_t sectors;                    /* 512-byte sectors */
    int      ssd;                        /* 1 = non-rotating */
};

/* --- ops table ------------------------------------------------------------ */
struct blk_ops {
    const char *name;                    /* "ahci" / "nvme" */
    int (*read)(void *priv, uint64_t lba, void *buf, size_t sectors);
    int (*write)(void *priv, uint64_t lba, const void *buf, size_t sectors);
    int (*identity)(void *priv, struct blk_identity *out);
    /* ATA SMART READ DATA (512-byte attribute page); NULL when unsupported. */
    int (*smart_read_data)(void *priv, void *out_512);
    /* Driver-specific log access (ATA SMART READ LOG / NVMe Get Log Page). */
    int (*smart_read_log)(void *priv, uint8_t page, void *buf, size_t sectors);
};

/* Register a probed device. Returns its index, or -1 when the table is full. */
int blk_register(const struct blk_ops *ops, void *priv);

/* --- public, driver-agnostic API (called from Rust) ----------------------- */
/* Open a drive by 0-based index; NULL when out of range. Every op also
 * accepts NULL as "the first registered drive" (the primary device). */
void *blk_open(size_t index);

/* Read/write SECTORS 512-byte sectors at LBA. Writes are gated by repair
 * mode in the kernel — the driver itself never refuses. */
int blk_read(void *dev, uint64_t lba, void *buf, size_t sectors);
int blk_write(void *dev, uint64_t lba, const void *buf, size_t sectors);

/* Driver name ("ahci"/"nvme") and decoded identity. */
const char *blk_name(void *dev);
int blk_identity(void *dev, struct blk_identity *out);

/* ATA SMART READ DATA; -1 when the device/driver has no such page. */
int blk_smart_read_data(void *dev, void *out_512);

/* Driver-specific log page: ATA SMART READ LOG (page = log) or NVMe
 * Get Log Page (page = log id, e.g. 0x02 = SMART/Health). */
int blk_smart_read_log(void *dev, uint8_t page, void *buf, size_t sectors);

#ifdef __cplusplus
}
#endif

#endif /* FANTUAN_DRIVER_H */
