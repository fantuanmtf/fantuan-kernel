/* rump_shim_lib.c - libkern subset, atomics and misc constants (ours).
 * The imported sources expect the NetBSD libkern/machine layer; on
 * x86_64-unknown-none that is these compiler-atomic and byte-loop fallbacks.
 */
#include <sys/types.h>
#include <sys/atomic.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/proc.h>
#include "rump_shim.h"

/* sys/libkern.h maps these to compiler builtins; the compiler still emits
 * calls to the external names for whole-struct copies, so define them. */
#undef memset
#undef memcpy
#undef memmove
#undef memcmp
#undef strlen
#undef strcmp
#undef strcpy
#undef strncpy
#undef strlcpy

unsigned long
_atomic_cas_ulong(volatile unsigned long *p, unsigned long o, unsigned long n)
{

	__atomic_compare_exchange_n(p, &o, n, 0,
	    __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
	return o;
}

void *
atomic_cas_ptr(volatile void *p, void *o, void *n)
{

	__atomic_compare_exchange_n((void *volatile *)p, &o, n, 0,
	    __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
	return o;
}

void *
atomic_swap_ptr(volatile void *p, void *n)
{

	return __atomic_exchange_n((void *volatile *)p, n, __ATOMIC_SEQ_CST);
}

void
atomic_inc_uint(volatile unsigned int *p)
{

	(void)__atomic_add_fetch(p, 1, __ATOMIC_SEQ_CST);
}

unsigned int
atomic_inc_uint_nv(volatile unsigned int *p)
{

	return __atomic_add_fetch(p, 1, __ATOMIC_SEQ_CST);
}

unsigned int
atomic_dec_uint_nv(volatile unsigned int *p)
{

	return __atomic_sub_fetch(p, 1, __ATOMIC_SEQ_CST);
}

void
atomic_dec_uint(volatile unsigned int *p)
{

	(void)__atomic_sub_fetch(p, 1, __ATOMIC_SEQ_CST);
}

void
membar_producer(void)
{

	__atomic_thread_fence(__ATOMIC_RELEASE);
}

void
membar_release(void)
{

	__atomic_thread_fence(__ATOMIC_RELEASE);
}

void *
memset(void *dst, int c, size_t len)
{
	unsigned char *d = dst;
	size_t i;

	for (i = 0; i < len; i++)
		d[i] = (unsigned char)c;
	return dst;
}

void *
memcpy(void *dst, const void *src, size_t len)
{
	unsigned char *d = dst;
	const unsigned char *s = src;
	size_t i;

	for (i = 0; i < len; i++)
		d[i] = s[i];
	return dst;
}

void *
memmove(void *dst, const void *src, size_t len)
{
	unsigned char *d = dst;
	const unsigned char *s = src;
	size_t i;

	if (d < s) {
		for (i = 0; i < len; i++)
			d[i] = s[i];
	} else {
		for (i = len; i > 0; i--)
			d[i - 1] = s[i - 1];
	}
	return dst;
}

size_t
strlen(const char *s)
{
	size_t n = 0;

	while (s[n] != '\0')
		n++;
	return n;
}

int
strcmp(const char *a, const char *b)
{

	while (*a != '\0' && *a == *b) {
		a++;
		b++;
	}
	return (unsigned char)*a - (unsigned char)*b;
}

char *
strcpy(char *dst, const char *src)
{
	char *d = dst;

	while ((*d++ = *src++) != '\0')
		continue;
	return dst;
}

int
memcmp(const void *a, const void *b, size_t len)
{
	const unsigned char *x = a;
	const unsigned char *y = b;
	size_t i;

	for (i = 0; i < len; i++) {
		if (x[i] != y[i])
			return (int)x[i] - (int)y[i];
	}
	return 0;
}

char *
strncpy(char *dst, const char *src, size_t len)
{
	char *d = dst;

	while (len > 0 && *src != '\0') {
		*d++ = *src++;
		len--;
	}
	while (len > 0) {
		*d++ = '\0';
		len--;
	}
	return dst;
}

size_t
strlcpy(char *dst, const char *src, size_t size)
{
	size_t n = strlen(src);

	if (size != 0) {
		size_t copy = n >= size ? size - 1 : n;

		memcpy(dst, src, copy);
		dst[copy] = '\0';
	}
	return n;
}

void
hash_value(void *dst, size_t dstlen, const void *src, size_t srclen)
{
	const unsigned char *p = src;
	uint32_t h = 2166136261u;
	size_t i;

	for (i = 0; i < srclen; i++) {
		h ^= p[i];
		h *= 16777619u;
	}
	if (dstlen > sizeof(h))
		dstlen = sizeof(h);
	memcpy(dst, &h, dstlen);
}

bool
get_expose_address(struct proc *p)
{

	(void)p;
	return false;
}

size_t coherency_unit = 64;

int
nullop(void *v)
{

	(void)v;
	return 0;
}
