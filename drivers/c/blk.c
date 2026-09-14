/* blk.c — block-device registry and ops dispatch (DESIGN.md §5).
 *
 * Storage drivers register a blk_ops table once their probe succeeds; the
 * Rust core sees only opaque handles. Keeping the dispatch here is what makes
 * the kernel storage-agnostic: VFS, boot repair and diagnostics never learn
 * whether the bytes came from AHCI or NVMe.
 */

#include "driver.h"

#define MAX_BLK_DEVS 4

struct blk_dev {
    const struct blk_ops *ops;
    void *priv;
};

static struct blk_dev g_devs[MAX_BLK_DEVS];
static int g_count;

int blk_register(const struct blk_ops *ops, void *priv)
{
    if (ops == NULL || priv == NULL || g_count >= MAX_BLK_DEVS) {
        return -1;
    }
    g_devs[g_count].ops = ops;
    g_devs[g_count].priv = priv;
    return g_count++;
}

/* Validate a handle: it must be one of the registered entries. A NULL handle
 * means "the first registered drive" — the kernel's primary device, which is
 * what the filesystem and diagnostic code paths pass. */
static struct blk_dev *entry_of(void *dev)
{
    int i;

    if (dev == NULL) {
        return g_count > 0 ? &g_devs[0] : NULL;
    }
    for (i = 0; i < g_count; i++) {
        if (dev == (void *)&g_devs[i]) {
            return &g_devs[i];
        }
    }
    return NULL;
}

void *blk_open(size_t index)
{
    if (index >= (size_t)g_count) {
        return NULL;
    }
    return (void *)&g_devs[index];
}

const char *blk_name(void *dev)
{
    struct blk_dev *e = entry_of(dev);
    return (e != NULL && e->ops->name != NULL) ? e->ops->name : "?";
}

int blk_identity(void *dev, struct blk_identity *out)
{
    struct blk_dev *e = entry_of(dev);
    if (e == NULL || out == NULL || e->ops->identity == NULL) {
        return -1;
    }
    return e->ops->identity(e->priv, out);
}

int blk_read(void *dev, uint64_t lba, void *buf, size_t sectors)
{
    struct blk_dev *e = entry_of(dev);
    if (e == NULL || e->ops->read == NULL) {
        return -1;
    }
    return e->ops->read(e->priv, lba, buf, sectors);
}

int blk_write(void *dev, uint64_t lba, const void *buf, size_t sectors)
{
    struct blk_dev *e = entry_of(dev);
    if (e == NULL || e->ops->write == NULL) {
        return -1;
    }
    return e->ops->write(e->priv, lba, buf, sectors);
}

int blk_smart_read_data(void *dev, void *out_512)
{
    struct blk_dev *e = entry_of(dev);
    if (e == NULL || e->ops->smart_read_data == NULL) {
        return -1;
    }
    return e->ops->smart_read_data(e->priv, out_512);
}

int blk_smart_read_log(void *dev, uint8_t page, void *buf, size_t sectors)
{
    struct blk_dev *e = entry_of(dev);
    if (e == NULL || e->ops->smart_read_log == NULL) {
        return -1;
    }
    return e->ops->smart_read_log(e->priv, page, buf, sectors);
}
