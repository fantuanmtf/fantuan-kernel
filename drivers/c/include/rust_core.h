/* rust_core.h — the narrow C interface exported by the Rust kernel
 * (DESIGN.md §5). Grows conservatively: every addition deliberately widens
 * the boundary and is documented there.
 *
 * Ownership contracts (M4.5):
 *   - k_alloc_page hands the C driver one 4K page; the driver owns it for
 *     its lifetime (no k_free in v1 — drivers are never unloaded).
 *   - k_log/k_log_hex are the only output channels; callable from any
 *     context but never from an interrupt handler.
 */
#ifndef FANTUAN_RUST_CORE_H
#define FANTUAN_RUST_CORE_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* --- logging ----------------------------------------------------------- */
void k_log(const char *s);
void k_log_hex(uint64_t v);

/* --- memory ------------------------------------------------------------ */
/* Physical -> virtual for MMIO/DMA addresses (PHYS_OFFSET alias, M2). */
uint64_t k_phys_to_virt(uint64_t phys);
/* One 4K page (contiguous, DMA-able, below 4 GiB). Returns the virtual
 * address; stores the physical address in *phys_out when non-NULL. */
void *k_alloc_page(uint64_t *phys_out);

/* --- time -------------------------------------------------------------- */
void k_delay_ms(uint64_t ms);

#ifdef __cplusplus
}
#endif

#endif /* FANTUAN_RUST_CORE_H */
