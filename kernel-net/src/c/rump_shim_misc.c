/* rump_shim_misc.c - small service shims the R4 IPv4 path needs (ours).
 * Entropy (a deterministic xorshift until R8 wires the real RNG), the
 * interrupt-context kmem pair, the address/lladdr printers used by the ARP
 * and ICMP diagnostic paths, the synchronous wqinput stand-in and the
 * sysctl/netstat tail-ends that are only reachable from userland tools
 * (R7+).
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/mbuf.h>
#include <sys/kmem.h>
#include <sys/sysctl.h>
#include <sys/lwp.h>
#include <sys/timevar.h>
#include <net/net_stats.h>
#include <net/if_dl.h>
#include <netinet/in.h>
#include <netinet/wqinput.h>
#include "rump_shim.h"

static const char misc_hexdigits[] = "0123456789abcdef";

/* Deterministic xorshift32; the boot tests only need a changing salt (ia_idsalt)
 * and an IP ID source.  Entropy is not a security claim here. */
uint32_t
cprng_fast32(void)
{
	static uint32_t state;
	uint32_t x = state;

	if (x == 0)
		x = (uint32_t)(fantuan_rump_ticks() ^ 0x9e3779b9u);
	x ^= x << 13;
	x ^= x >> 17;
	x ^= x << 5;
	state = x;
	return x;
}

void *
kmem_intr_alloc(size_t size, km_flag_t flags)
{

	/* No interrupt-context allocator before F2; callers run in task
	 * context, so the normal kmem path is safe. */
	return kmem_alloc(size, flags);
}

void
kmem_intr_free(void *p, size_t size)
{

	kmem_free(p, size);
}

int
in_print(char *buf, size_t len, const struct in_addr *addr)
{
	const uint8_t *p = (const uint8_t *)&addr->s_addr;

	return snprintf(buf, len, "%u.%u.%u.%u", p[0], p[1], p[2], p[3]);
}

int
sin_print(char *buf, size_t len, const void *v)
{
	const struct sockaddr_in *sin = v;

	return in_print(buf, len, &sin->sin_addr);
}

char *
intoa(uint32_t addr)
{
	static char buf[16];
	struct in_addr in;

	in.s_addr = addr;
	(void)in_print(buf, sizeof(buf), &in);
	return buf;
}

char *
lla_snprintf(char *dst, size_t dst_len, const void *src, size_t src_len)
{
	const uint8_t *sp = src;
	size_t i;

	for (i = 0; i < src_len; i++) {
		if (dst_len < 4)
			break;
		*dst++ = misc_hexdigits[sp[i] >> 4];
		*dst++ = misc_hexdigits[sp[i] & 0xf];
		if (i + 1 < src_len)
			*dst++ = ':';
		dst_len -= 3;
	}
	*dst = '\0';
	return dst;
}

/* wqinput(9) is a workqueue-deferred input hook; the single-CPU adapter
 * runs it synchronously in the input task. */
struct wqinput {
	void (*wq_func)(struct mbuf *, int, int);
};

struct wqinput *
wqinput_create(const char *name, void (*func)(struct mbuf *, int, int))
{
	struct wqinput *wq;

	(void)name;
	wq = kmem_zalloc(sizeof(*wq), KM_SLEEP);
	if (wq != NULL)
		wq->wq_func = func;
	return wq;
}

void
wqinput_input(struct wqinput *wq, struct mbuf *m, int off, int proto)
{

	if (wq != NULL && wq->wq_func != NULL)
		wq->wq_func(m, off, proto);
	else
		m_freem(m);
}

int
sysctl_copyinstr(struct lwp *l, const void *src, void *dst, size_t maxlen,
    size_t *lenp)
{

	(void)l;
	if (src == NULL || dst == NULL)
		return EFAULT;
	if (lenp != NULL)
		*lenp = strlcpy(dst, src, maxlen);
	return 0;
}

int
netstat_sysctl(percpu_t *stat, u_int nctrs, const int *name, u_int namelen,
    void *oldp, size_t *oldlenp, const void *newp, size_t newlen,
    const int *oname, struct lwp *l, const struct sysctlnode *rnode)
{

	(void)stat;
	(void)nctrs;
	(void)name;
	(void)namelen;
	(void)oldp;
	(void)oldlenp;
	(void)newp;
	(void)newlen;
	(void)oname;
	(void)l;
	(void)rnode;
	return 0;
}

int
ppsratecheck(struct timeval *last, int *count, int interval)
{

	/* ICMP error rate limiting: the offline boot tests never emit enough
	 * errors for a limiter to matter; accept every call and let the call
	 * sites count (R7 restores the real pps accounting). */
	(void)last;
	(void)count;
	(void)interval;
	return 1;
}
