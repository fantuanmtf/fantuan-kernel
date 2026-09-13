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

/* Write SECTORS 512-byte sectors from BUF starting at LBA (M7.5b). The
 * kernel gates this behind repair mode — never call it casually. */
int blk_write(void *dev, uint64_t lba, const void *buf, size_t sectors);

/* Open a drive by 0-based index. Returns a per-drive opaque handle, or
 * NULL if the index is out of range. Currently only index 0 is valid. */
void *blk_open(size_t index);

/* Retrieve the 512-byte IDENTIFY (ATA) or Identify Namespace (NVMe) page
 * for the drive pointed to by DEV. Returns 0 on success, -1 on error. */
int blk_identify(void *dev, void *out_512);

/* Issue SMART READ DATA and write the 512-byte result into OUT_512.
 * Returns 0 on success, -1 on error (drive error or invalid handle). */
int blk_smart_read_data(void *dev, void *out_512);

/* Issue SMART READ LOG for log page LOG_PAGE.  The data is written to BUF
 * as SECTORS * 512 bytes; the caller supplies a buffer of that size.
 * Returns 0 on success, -1 on error. */
int blk_smart_read_log(void *dev, uint8_t log_page, void *buf, size_t sectors);

#ifdef __cplusplus
}
#endif

#endif /* FANTUAN_DRIVER_H */
