/* driver.h — the C driver ops interface (DESIGN.md §5).
 * v1: block-device read path only, polling, read-only. Writes and IRQ
 * registration arrive with the first writable filesystem (M6).
 */
#ifndef FANTUAN_DRIVER_H
#define FANTUAN_DRIVER_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Read SECTORS 512-byte sectors starting at LBA into BUF.
 * Returns 0 on success, negative on error. Polling; not IRQ-driven. */
int blk_read(void *dev, uint64_t lba, void *buf, size_t sectors);

#ifdef __cplusplus
}
#endif

#endif /* FANTUAN_DRIVER_H */
