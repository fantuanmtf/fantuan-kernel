/* libc-fantuan — sys/wait.h (P1): declarations present, ENOSYS until the
 * P2 fork/exec/wait4 batch. */
#ifndef _SYS_WAIT_H
#define _SYS_WAIT_H

#include <sys/types.h>

#define WNOHANG 1
#define WUNTRACED 2
#define WCONTINUED 8

#define WIFEXITED(status) (((status)&0x7f) == 0)
#define WEXITSTATUS(status) (((status)&0xff00) >> 8)
#define WIFSIGNALED(status) (((status)&0x7f) != 0 && ((status)&0x7f) != 0x7f)
#define WTERMSIG(status) ((status)&0x7f)
#define WIFSTOPPED(status) (((status)&0xff) == 0x7f)
#define WSTOPSIG(status) WEXITSTATUS(status)

typedef int idtype_t;
#define P_ALL 0
#define P_PID 1
#define P_PGID 2

pid_t wait(int *status);
pid_t waitpid(pid_t pid, int *status, int options);
pid_t wait4(pid_t pid, int *status, int options, void *rusage);

#endif /* _SYS_WAIT_H */
