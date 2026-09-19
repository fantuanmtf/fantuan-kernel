/* fantuan adaptation layer for the NetBSD rump slice (M11 R2).
 * Internal interface between the adapter C files and the Rust half of
 * kernel-net; ours, not upstream. */
#ifndef FANTUAN_RUMP_SHIM_H
#define FANTUAN_RUMP_SHIM_H

#include <sys/types.h>

void fantuan_rump_log(const void *, size_t);
void fantuan_rump_panic(const void *, size_t) __attribute__((noreturn));
void *fantuan_rump_pages_alloc(size_t);
void fantuan_rump_pages_free(void *, size_t);
uint64_t fantuan_rump_ticks(void);
uint64_t fantuan_rump_physmem_pages(void);
uint64_t fantuan_rump_switch_count(void);

void rump_shim_init(void);
void rump_shim_tick(void);
void rump_softint_dispatch(void);
int rump_selftest_poll(void);

#endif
