/* rump_shim_sysctl.c - read-only sysctl stubs and bounded copyout (ours).
 * The imported sources register sysctl nodes during init; R2 accepts and
 * discards the registration and never exposes the tree to userland.
 */
#include <sys/types.h>
#include <sys/errno.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/sysctl.h>
#include <sys/lwp.h>
#include <stdarg.h>
#include "rump_shim.h"

/* sys/sysctl.h wraps the variadic entry point in a type-verifying macro. */
#undef sysctl_createv

int
sysctl_createv(struct sysctllog **logp, int ctl_flags,
    const struct sysctlnode **rnode, const struct sysctlnode **cnode,
    int flags, int type, const char *name, const char *desc, sysctlfn fn,
    u_quad_t qv, void *newp, size_t newlen, ...)
{
	va_list ap;
	int v;

	(void)logp;
	(void)ctl_flags;
	(void)rnode;
	(void)cnode;
	(void)flags;
	(void)type;
	(void)name;
	(void)desc;
	(void)fn;
	(void)qv;
	(void)newp;
	(void)newlen;

	va_start(ap, newlen);
	do {
		v = va_arg(ap, int);
	} while (v != CTL_EOL);
	va_end(ap);
	return 0;
}

int
sysctl_lookup(SYSCTLFN_ARGS)
{

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
sysctl_query(SYSCTLFN_ARGS)
{

	return sysctl_lookup(SYSCTLFN_CALL(rnode));
}

int
sysctl_copyout(struct lwp *l, const void *src, void *dst, size_t len)
{

	(void)l;
	if (dst == NULL)
		return EFAULT;
	memcpy(dst, src, len);
	return 0;
}

void
sysctl_unlock(void)
{
}

void
sysctl_relock(void)
{
}

int
copyout(const void *src, void *dst, size_t len)
{

	if (dst == NULL)
		return EFAULT;
	memcpy(dst, src, len);
	return 0;
}

int
copyin(const void *src, void *dst, size_t len)
{

	if (src == NULL || dst == NULL)
		return EFAULT;
	memcpy(dst, src, len);
	return 0;
}

void
sysctl_teardown(struct sysctllog **logp)
{

	if (logp != NULL)
		*logp = NULL;
}
