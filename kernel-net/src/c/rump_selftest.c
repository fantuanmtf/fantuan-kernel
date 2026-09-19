/* rump_selftest.c - in-kernel proof for the R2 adaptation layer (ours).
 * One poll per softintd-style timeslice; the Rust half runs this from a
 * bounded kernel task. Steps: mbuf MGET/MCLGET plus cluster refcounts,
 * pool get/put with statistics, and a PIT-driven callout that re-arms
 * itself CALL_TARGET times.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/mbuf.h>
#include <sys/pool.h>
#include <sys/callout.h>
#include <sys/kernel.h>
#include "rump_shim.h"

#define CALL_TARGET 20

static struct pool test_pool;
static struct callout test_callout;

static int phase;
static int allocs;
static int frees;
static int pool_gets;
static int pool_puts;
static volatile int fires;
static int deadline;

static void
fail(const char *step)
{

	printf("rump: self-test FAILED (%s)\n", step);
	printf("rump: counters mbuf allocs=%d frees=%d pool get=%d put=%d "
	    "free=%u callout fires=%d ticks=%d\n",
	    allocs, frees, pool_gets, pool_puts, test_pool.pr_nitems,
	    fires, getticks());
}

static int
mbuf_step(void)
{
	struct mbuf *m, *n;
	char buf[4];
	int i;

	allocs = 0;
	frees = 0;

	for (i = 0; i < 8; i++) {
		MGET(m, M_DONTWAIT, MT_DATA);
		if (m == NULL)
			return 0;
		allocs++;
		memcpy(m->m_data, "rump", 4);
		m->m_len = 4;
		m_free(m);
		frees++;
	}

	for (i = 0; i < 4; i++) {
		m = m_gethdr(M_DONTWAIT, MT_DATA);
		if (m == NULL)
			return 0;
		allocs++;
		MCLGET(m, M_DONTWAIT);
		if ((m->m_flags & M_EXT) == 0) {
			m_free(m);
			return 0;
		}
		if (m->m_ext.ext_refcnt != 1) {
			m_freem(m);
			return 0;
		}
		memcpy(m->m_data, "rump", 4);
		m->m_len = 4;

		n = m_copym(m, 0, M_COPYALL, M_DONTWAIT);
		if (n == NULL) {
			m_freem(m);
			return 0;
		}
		if (m->m_ext.ext_refcnt != 2) {
			m_freem(n);
			m_freem(m);
			return 0;
		}
		m_copydata(n, 0, 4, buf);
		if (buf[0] != 'r' || buf[1] != 'u' || buf[2] != 'm' ||
		    buf[3] != 'p') {
			m_freem(n);
			m_freem(m);
			return 0;
		}
		m_freem(n);
		if (m->m_ext.ext_refcnt != 1) {
			m_freem(m);
			return 0;
		}
		m_freem(m);
		frees++;
	}

	return allocs == frees;
}

static int
pool_step(void)
{
	void *objs[8];
	unsigned int before;
	int i;

	pool_init(&test_pool, 128, 64, 0, 0, "rumptest", NULL, IPL_VM);
	before = pool_nget(&test_pool);
	pool_gets = 0;
	pool_puts = 0;

	for (i = 0; i < 8; i++) {
		objs[i] = pool_get(&test_pool, PR_NOWAIT);
		if (objs[i] == NULL)
			return 0;
	}
	pool_gets = (int)(pool_nget(&test_pool) - before);
	for (i = 0; i < 8; i++)
		pool_put(&test_pool, objs[i]);
	pool_puts = (int)pool_nput(&test_pool);

	return pool_gets == 8 && pool_puts == 8 &&
	    test_pool.pr_nitems >= 8;
}

static void
callout_cb(void *arg)
{

	(void)arg;
	fires++;
	if (fires < CALL_TARGET)
		callout_reset(&test_callout, 2, callout_cb, NULL);
}

int
rump_selftest_poll(void)
{

	switch (phase) {
	case 0:
		phase = 9;
		if (!mbuf_step()) {
			fail("mbuf");
			return 1;
		}
		printf("rump: mbuf self-test ok (allocs=%d frees=%d)\n",
		    allocs, frees);
		phase = 1;
		return 0;
	case 1:
		phase = 9;
		if (!pool_step()) {
			fail("pool");
			return 1;
		}
		printf("rump: pool self-test ok\n");
		phase = 2;
		return 0;
	case 2:
		fires = 0;
		phase = 3;
		callout_init(&test_callout, CALLOUT_MPSAFE);
		callout_reset(&test_callout, 2, callout_cb, NULL);
		deadline = getticks() + 200;
		return 0;
	case 3:
		if (fires >= CALL_TARGET) {
			callout_stop(&test_callout);
			callout_destroy(&test_callout);
			printf("rump: callout self-test ok (fires=%d)\n",
			    fires);
			phase = 4;
			return 1;
		}
		if (getticks() > deadline) {
			callout_stop(&test_callout);
			fail("callout");
			phase = 9;
			return 1;
		}
		return 0;
	default:
		return 1;
	}
}
