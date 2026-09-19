/* rump_shim_ksock.c - socket-layer services the imported socket/TCP code
 * calls but the M11 slice does not provide (ours).  These are the select/
 * kqueue, credential, lo-fi, UVM-loan and accept-filter edges of
 * uipc_socket.c/uipc_socket2.c.  The kernel socket client polls, so the
 * notification paths are no-ops; UVM loaning is disabled (sock_loan_thresh
 * is -1) and its entry points refuse so sosend() always copies.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/errno.h>
#include <sys/uio.h>
#include <sys/poll.h>
#include <sys/select.h>
#include <sys/selinfo.h>
#include <sys/socketvar.h>
#include <sys/kauth.h>
#include <sys/uidinfo.h>
#include <sys/ioctl.h>
#include <sys/compat_stub.h>
#include <uvm/uvm_extern.h>
#include <uvm/uvm_loan.h>
#include <uvm/uvm_page.h>
#include <uvm/uvm_pmap.h>
#include "rump_shim.h"

/* select(2)/kqueue(9) are outside the slice: the kernel client polls. */
void
selinit(struct selinfo *si)
{

	(void)si;
}

void
seldestroy(struct selinfo *si)
{

	(void)si;
}

void
selrecord(struct lwp *selector, struct selinfo *si)
{

	(void)selector;
	(void)si;
}

void
selrecord_knote(struct selinfo *si, struct knote *kn)
{

	(void)si;
	(void)kn;
}

bool
selremove_knote(struct selinfo *si, struct knote *kn)
{

	(void)si;
	(void)kn;
	return false;
}

void
selnotify(struct selinfo *si, int events, long hint)
{

	(void)si;
	(void)events;
	(void)hint;
}

/* SIGIO/SIGURG owners are not tracked before M14 (no descriptor table). */
void
fownsignal(int pgid, int sig, int code, int band, void *arg)
{

	(void)pgid;
	(void)sig;
	(void)code;
	(void)band;
	(void)arg;
}

/* One sentinel credential: kauth is "no listeners -> ALLOW" and the kernel
 * client never inspects the credential. */
void
kauth_cred_hold(kauth_cred_t cred)
{

	(void)cred;
}

void
kauth_cred_free(kauth_cred_t cred)
{

	(void)cred;
}

uid_t
kauth_cred_geteuid(kauth_cred_t cred)
{

	(void)cred;
	return 0;
}

gid_t
kauth_cred_getegid(kauth_cred_t cred)
{

	(void)cred;
	return 0;
}

uid_t
kauth_cred_getuid(kauth_cred_t cred)
{

	(void)cred;
	return 0;
}

int
proc_uidmatch(kauth_cred_t a, kauth_cred_t b)
{

	(void)a;
	(void)b;
	return 1;
}

/* NTIME compat hook storage (compat_stub.c upstream). */
struct uipc_socket_50_sbts_hook_t uipc_socket_50_sbts_hook;

/* uidinfo: one static record; so_uidinfo only feeds chgsbsize(). */
static struct uidinfo ksock_uidinfo;

struct uidinfo *
uid_find(uid_t uid)
{

	(void)uid;
	return &ksock_uidinfo;
}

int
chgsbsize(struct uidinfo *uip, u_long *hiwat, u_long cc, rlim_t max)
{

	(void)uip;
	if (max != RLIM_INFINITY && cc > max)
		return 0;
	*hiwat = cc;
	return 1;
}

/* uiomove: the kernel client passes UIO_SYSSPACE uios only (uio_vmspace is
 * not used by the copy paths), so a plain iovec walk is exact. */
int
uiomove(void *cp, size_t n, struct uio *uio)
{
	uint8_t *p = cp;

	while (n > 0 && uio->uio_resid > 0) {
		struct iovec *iov = uio->uio_iov;
		size_t len = iov->iov_len;

		if (len == 0) {
			uio->uio_iov++;
			uio->uio_iovcnt--;
			continue;
		}
		if (len > n)
			len = n;
		if (uio->uio_rw == UIO_READ)
			memcpy(iov->iov_base, p, len);
		else
			memcpy(p, iov->iov_base, len);
		iov->iov_base = (char *)iov->iov_base + len;
		iov->iov_len -= len;
		p += len;
		n -= len;
		uio->uio_resid -= len;
		uio->uio_offset += (off_t)len;
	}
	return n == 0 ? 0 : EFAULT;
}

/* UVM loaning: the adapter has no loanable map, so uvm_loan() fails and
 * sosend() falls back to the copy path. */
int
uvm_loan(struct vm_map *map, vaddr_t start, vsize_t len, struct vm_page **pgs,
    int flags)
{

	(void)map;
	(void)start;
	(void)len;
	(void)pgs;
	(void)flags;
	return EINVAL;
}

void
uvm_unloan(struct vm_page **pgs, int npages, int flags)
{

	(void)pgs;
	(void)npages;
	(void)flags;
}

struct vm_map *kernel_map;
struct uvmexp uvmexp;

static struct pmap ksock_kernel_pmap;
struct pmap *const kernel_pmap_ptr = &ksock_kernel_pmap;

void
pmap_kenter_pa(vaddr_t va, paddr_t pa, vm_prot_t prot, u_int flags)
{

	(void)va;
	(void)pa;
	(void)prot;
	(void)flags;
}

void
pmap_kremove(vaddr_t va, vsize_t len)
{

	(void)va;
	(void)len;
}

void
pmap_update(pmap_t pm)
{

	(void)pm;
}

/* accept filters are stopped by default; SO_ACCEPTFILTER is refused. */
int
accept_filt_clear(struct socket *so)
{

	(void)so;
	return 0;
}

int
accept_filt_setopt(struct socket *so, const struct sockopt *sopt)
{

	(void)so;
	(void)sopt;
	return EOPNOTSUPP;
}

int
accept_filt_getopt(struct socket *so, struct sockopt *sopt)
{

	(void)so;
	(void)sopt;
	return EOPNOTSUPP;
}
