/* fantuan adaptation shim: x86_64 byte swapping (not upstream NetBSD). */
#ifndef FANTUAN_MACHINE_BSWAP_H
#define FANTUAN_MACHINE_BSWAP_H

static __inline uint16_t bswap16(uint16_t x) { return __builtin_bswap16(x); }
static __inline uint32_t bswap32(uint32_t x) { return __builtin_bswap32(x); }
static __inline uint64_t bswap64(uint64_t x) { return __builtin_bswap64(x); }

#endif
