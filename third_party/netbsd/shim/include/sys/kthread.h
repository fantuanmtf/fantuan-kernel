/* fantuan adaptation shim: sys/kthread.h - compile-only kthread prototypes
 * (ours, not upstream).  The socket layer's sopendfree thread is not
 * created (sosend loaning is off), so kthread_create is never called. */
#ifndef FANTUAN_SYS_KTHREAD_H
#define FANTUAN_SYS_KTHREAD_H

#include <sys/types.h>

struct cpu_info;
struct lwp;
typedef struct lwp lwp_t;

#define KTHREAD_MPSAFE	0x02	/* do not acquire kernel_lock */

int	kthread_create(int, int, struct cpu_info *,
	    void (*)(void *), void *, lwp_t **, const char *, ...);
void	kthread_exit(int);
int	kthread_join(lwp_t *);

#endif
