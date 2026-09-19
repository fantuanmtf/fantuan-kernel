/* rump_shim_mobj.c - shareable mutex objects over the bump allocator
 * (ours).  The MI lock code calls mutex_obj_alloc/hold/free for softnet_lock
 * and the callout CPU locks; all TCP sockets hold a reference to
 * softnet_lock, so freeing must be reference-counted (freeing it on the
 * first socket close would corrupt every other socket).  Single-CPU task
 * context, so a plain table needs no lock of its own. */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/kmem.h>
#include <sys/mutex.h>
#include "rump_shim.h"

#define MOBJ_MAX 64

static struct {
	kmutex_t *mo_mutex;
	unsigned mo_refs;
} mobj_table[MOBJ_MAX];

static int
mobj_slot(kmutex_t *m)
{
	int i;

	for (i = 0; i < MOBJ_MAX; i++)
		if (mobj_table[i].mo_mutex == m)
			return i;
	return -1;
}

kmutex_t *
mutex_obj_alloc(kmutex_type_t type, int ipl)
{
	kmutex_t *m = kmem_alloc(sizeof(*m), KM_SLEEP);
	int i;

	if (m == NULL)
		panic("mutex_obj_alloc: out of memory");
	mutex_init(m, type, ipl);
	i = mobj_slot(NULL);
	KASSERT(i >= 0);
	mobj_table[i].mo_mutex = m;
	mobj_table[i].mo_refs = 1;
	return m;
}

void
mutex_obj_hold(kmutex_t *m)
{
	int i;

	if (m == NULL)
		return;
	i = mobj_slot(m);
	if (i >= 0) {
		mobj_table[i].mo_refs++;
	} else {
		i = mobj_slot(NULL);
		KASSERT(i >= 0);
		mobj_table[i].mo_mutex = m;
		mobj_table[i].mo_refs = 2;
	}
}

bool
mutex_obj_free(kmutex_t *m)
{
	int i;

	if (m == NULL)
		return false;
	i = mobj_slot(m);
	if (i < 0)
		return false;
	if (--mobj_table[i].mo_refs > 0)
		return false;
	mutex_destroy(m);
	kmem_free(m, sizeof(*m));
	mobj_table[i].mo_mutex = NULL;
	mobj_table[i].mo_refs = 0;
	return true;
}
