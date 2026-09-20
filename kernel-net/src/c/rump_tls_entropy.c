/* rump_tls_entropy.c - mbedTLS platform glue for kernel-net (ours, M11 R8).
 * The kernel has no libc, no filesystem, no threads and no wall clock:
 *   - memory: mbedtls calloc/free route to the adapter kmem arena;
 *   - time: mbedtls_time() and mbedtls_ms_time() return PIT ticks (100 Hz);
 *   - entropy: a SHA-256 counter CSPRNG seeded at boot from RDRAND when the
 *     CPU exposes it (CPUID.01H:ECX[30]), mixed with RDTSC and the PIT tick.
 * mbedtls_hardware_poll() draws from that CSPRNG; mbedTLS entropy and
 * CTR_DRBG condition it further.  No security claim on RDRAND-less VMs.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/kmem.h>
#include "rump_shim.h"

#include "mbedtls/platform.h"
#include "mbedtls/platform_time.h"
#include "mbedtls/sha256.h"

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#define TLS_SEED_LEN 32

static uint8_t tls_state[TLS_SEED_LEN];
static uint8_t tls_out[TLS_SEED_LEN];
static size_t tls_out_pos = TLS_SEED_LEN;
static uint64_t tls_counter;
static int tls_seeded;

/* strchr() is only used by mbedTLS x509 parsing; the adapter has no libc. */
char *
strchr(const char *s, int c)
{
	for (; *s != '\0'; s++)
		if (*s == (char)c)
			return (char *)(uintptr_t)s;
	return (c == 0) ? (char *)(uintptr_t)s : NULL;
}

static uint64_t
tls_rdtsc(void)
{
	uint32_t lo, hi;

	__asm__ volatile("rdtsc" : "=a"(lo), "=d"(hi));
	return ((uint64_t)hi << 32) | lo;
}

static int
tls_have_rdrand(void)
{
	uint32_t eax = 1, ebx, ecx, edx;

	__asm__ volatile("cpuid"
	    : "+a"(eax), "=b"(ebx), "=c"(ecx), "=d"(edx));
	return (ecx >> 30) & 1;
}

static int
tls_rdrand64(uint64_t *out)
{
	unsigned char ok = 0;

	__asm__ volatile("rdrand %0; setc %1" : "=r"(*out), "=qm"(ok));
	return ok;
}

static void
tls_mix(uint8_t *state, const void *data, size_t len, uint64_t counter)
{
	uint8_t block[48];

	memcpy(block, state, TLS_SEED_LEN);
	memcpy(block + TLS_SEED_LEN, &counter, sizeof(counter));
	memcpy(block + TLS_SEED_LEN + sizeof(counter), data,
	    len < 8 ? len : 8);
	mbedtls_sha256(block, sizeof(block), state, 0);
}

void
rump_tls_seed(void)
{
	uint64_t word;
	unsigned i;

	if (tls_seeded)
		return;
	memset(tls_state, 0, sizeof(tls_state));
	if (tls_have_rdrand()) {
		for (i = 0; i < 32; i++) {
			if (!tls_rdrand64(&word))
				word = tls_rdtsc() ^ fantuan_rump_ticks() ^ i;
			tls_mix(tls_state, &word, sizeof(word), i);
		}
	} else {
		for (i = 0; i < 32; i++) {
			uint64_t mix = tls_rdtsc() ^
			    (fantuan_rump_ticks() << 17) ^
			    (uint64_t)(uintptr_t)&word ^ i;
			tls_mix(tls_state, &mix, sizeof(mix), i);
		}
	}
	tls_counter = 0;
	tls_out_pos = TLS_SEED_LEN;
	tls_seeded = 1;
}

/* mbedTLS entropy hardware source: pull bytes from the counter CSPRNG. */
int
mbedtls_hardware_poll(void *data, unsigned char *output, size_t len,
    size_t *olen)
{
	size_t want = len;

	(void)data;
	while (len > 0) {
		size_t take;

		if (tls_out_pos >= TLS_SEED_LEN) {
			uint64_t c = ++tls_counter;
			uint8_t block[40];

			memcpy(block, tls_state, TLS_SEED_LEN);
			memcpy(block + TLS_SEED_LEN, &c, sizeof(c));
			mbedtls_sha256(block, sizeof(block), tls_out, 0);
			memcpy(tls_state, tls_out, TLS_SEED_LEN);
			tls_out_pos = 0;
		}
		take = TLS_SEED_LEN - tls_out_pos;
		if (take > len)
			take = len;
		memcpy(output, tls_out + tls_out_pos, take);
		tls_out_pos += take;
		output += take;
		len -= take;
	}
	*olen = want;
	return 0;
}

/* mbedTLS platform time: PIT seconds and milliseconds since boot. */
static mbedtls_time_t
tls_time(mbedtls_time_t *t)
{
	mbedtls_time_t now = (mbedtls_time_t)(fantuan_rump_ticks() / 100);

	if (t != NULL)
		*t = now;
	return now;
}

mbedtls_ms_time_t
mbedtls_ms_time(void)
{

	return (mbedtls_ms_time_t)(fantuan_rump_ticks() * 10);
}

/* mbedTLS platform memory: the adapter kmem arena (bump; no recycling). */
static void *
tls_calloc(size_t n, size_t size)
{

	if (n != 0 && size > (size_t)-1 / n)
		return NULL;
	return kmem_zalloc(n * size, KM_SLEEP);
}

static void
tls_free(void *p)
{

	kmem_free(p, 0);
}

int
rump_tls_platform_init(void)
{

	(void)mbedtls_platform_set_calloc_free(tls_calloc, tls_free);
	(void)mbedtls_platform_set_snprintf(snprintf);
	(void)mbedtls_platform_set_time(tls_time);
	return 0;
}
