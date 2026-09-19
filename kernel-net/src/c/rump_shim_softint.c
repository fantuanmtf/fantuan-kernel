/* rump_shim_softint.c - deferred callbacks for the callout wheel (ours).
 * softint_schedule() only marks a handler pending; the kernel-net softintd
 * task drains the table through rump_softint_dispatch(), so callout
 * callbacks never run in interrupt context.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <machine/intr.h>
#include "rump_shim.h"

#define SOFTINT_MAX 8

struct softint_handler {
	void (*fn)(void *);
	void *arg;
	volatile int pending;
};

static struct softint_handler softints[SOFTINT_MAX];

void *
softint_establish(u_int flags, void (*fn)(void *), void *arg)
{
	int i;

	(void)flags;
	for (i = 0; i < SOFTINT_MAX; i++) {
		if (softints[i].fn == NULL) {
			softints[i].arg = arg;
			softints[i].pending = 0;
			softints[i].fn = fn;
			return &softints[i];
		}
	}
	return NULL;
}

void
softint_disestablish(void *cookie)
{

	if (cookie != NULL)
		((struct softint_handler *)cookie)->fn = NULL;
}

void
softint_schedule(void *cookie)
{
	struct softint_handler *h = cookie;

	if (h != NULL)
		h->pending = 1;
}

void
rump_softint_dispatch(void)
{
	int i;

	for (i = 0; i < SOFTINT_MAX; i++) {
		if (softints[i].fn != NULL && softints[i].pending) {
			void (*fn)(void *) = softints[i].fn;
			void *arg = softints[i].arg;

			softints[i].pending = 0;
			fn(arg);
		}
	}
}
