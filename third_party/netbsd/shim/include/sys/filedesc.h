/* fantuan adaptation shim: sys/filedesc.h - compile-only fd-layer types
 * (ours, not upstream).  uipc_socket2.c includes this header; the fd code
 * itself is compiled out (no DDB) or unreachable (fsocreate). */
#ifndef FANTUAN_SYS_FILEDESC_H
#define FANTUAN_SYS_FILEDESC_H

#include <sys/types.h>

struct lwp;
struct file;

typedef struct filedesc filedesc_t;

int	fd_allocfile(struct file **, int *);
void	fd_set_exclose(struct lwp *, int, bool);

#endif
