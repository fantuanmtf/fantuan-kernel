/* libc-fantuan — signals (P2): the set helpers, sigaction/sigprocmask with
 * the kernel restorer contract, kill/killpg/raise and sigsuspend.
 *
 * The kernel pushes `[restorer][SignalFrame]` on the user stack before a
 * handler runs; the handler returns into __fantuan_sigreturn_trampoline
 * (assembly, no stack use) which issues SYS_SIGRETURN. */
#include <errno.h>
#include <signal.h>
#include <string.h>
#include <unistd.h>
#include <fantuan/abi.h>

long __fantuan_raw(long n, long a1, long a2, long a3, long a4, long a5);
void __fantuan_sigreturn_trampoline(void);

/* Kernel contract: the restorer is required and must run on the frame. */
static void set_restorer(struct sigaction *act)
{
    act->sa_restorer = __fantuan_sigreturn_trampoline;
}

int sigemptyset(sigset_t *set)
{
    if (!set) {
        errno = EINVAL;
        return -1;
    }
    *set = 0;
    return 0;
}

int sigfillset(sigset_t *set)
{
    if (!set) {
        errno = EINVAL;
        return -1;
    }
    *set = ~(sigset_t)0;
    return 0;
}

int sigaddset(sigset_t *set, int signum)
{
    if (!set || signum < 1 || signum >= NSIG) {
        errno = EINVAL;
        return -1;
    }
    *set |= (sigset_t)1 << signum;
    return 0;
}

int sigdelset(sigset_t *set, int signum)
{
    if (!set || signum < 1 || signum >= NSIG) {
        errno = EINVAL;
        return -1;
    }
    *set &= ~((sigset_t)1 << signum);
    return 0;
}

int sigismember(const sigset_t *set, int signum)
{
    if (!set || signum < 1 || signum >= NSIG) {
        errno = EINVAL;
        return -1;
    }
    return (*set >> signum) & 1;
}

int sigaction(int signum, const struct sigaction *act, struct sigaction *oldact)
{
    struct sigaction tmp;
    struct sigaction *in = NULL;
    if (act) {
        tmp = *act;
        set_restorer(&tmp);
        in = &tmp;
    }
    long r = __fantuan_raw(FANTUAN_SYS_SIGACTION, signum, (long)in, (long)oldact, 0, 0);
    if (r < 0 && r > -4096) {
        errno = (int)-r;
        return -1;
    }
    return (int)r;
}

void (*signal(int signum, void (*handler)(int)))(int)
{
    struct sigaction act;
    struct sigaction old;
    memset(&act, 0, sizeof(act));
    act.sa_handler = handler;
    act.sa_flags = SA_RESTART;
    if (sigaction(signum, &act, &old) != 0) {
        return SIG_ERR;
    }
    return old.sa_handler;
}

int sigprocmask(int how, const sigset_t *set, sigset_t *oldset)
{
    long r = __fantuan_raw(FANTUAN_SYS_SIGPROCMASK, how, (long)set, (long)oldset, 0, 0);
    if (r < 0 && r > -4096) {
        errno = (int)-r;
        return -1;
    }
    return (int)r;
}

int sigsuspend(const sigset_t *mask)
{
    long r = __fantuan_raw(FANTUAN_SYS_SIGSUSPEND, mask ? (long)*mask : 0, 0, 0, 0, 0);
    if (r < 0 && r > -4096) {
        errno = (int)-r;
        return -1;
    }
    errno = EINTR; /* POSIX: sigsuspend always reports interruption */
    return -1;
}

int kill(pid_t pid, int sig)
{
    long r = __fantuan_raw(FANTUAN_SYS_KILL, pid, sig, 0, 0, 0);
    if (r < 0 && r > -4096) {
        errno = (int)-r;
        return -1;
    }
    return (int)r;
}

int killpg(int pgrp, int sig)
{
    if (pgrp <= 0) {
        errno = EINVAL;
        return -1;
    }
    return kill(-pgrp, sig);
}

int raise(int sig)
{
    return kill(getpid(), sig);
}

char *strsignal(int sig)
{
    switch (sig) {
    case SIGHUP: return (char *)"Hangup";
    case SIGINT: return (char *)"Interrupt";
    case SIGQUIT: return (char *)"Quit";
    case SIGKILL: return (char *)"Killed";
    case SIGPIPE: return (char *)"Broken pipe";
    case SIGALRM: return (char *)"Alarm clock";
    case SIGTERM: return (char *)"Terminated";
    case SIGCHLD: return (char *)"Child exited";
    case SIGSEGV: return (char *)"Segmentation fault";
    case SIGFPE: return (char *)"Floating point exception";
    default: return (char *)"Unknown signal";
    }
}
