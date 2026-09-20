/* libc-fantuan — signals (P1): the sigset_t helpers are real; delivery,
 * handlers and process groups arrive with the P2 fork/exec batch. */
#include <errno.h>
#include <signal.h>
#include <string.h>

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

void (*signal(int signum, void (*handler)(int)))(int)
{
    (void)signum;
    (void)handler;
    errno = ENOSYS;
    return SIG_ERR;
}

int sigaction(int signum, const struct sigaction *act, struct sigaction *oldact)
{
    (void)signum;
    (void)act;
    (void)oldact;
    errno = ENOSYS;
    return -1;
}

int sigprocmask(int how, const sigset_t *set, sigset_t *oldset)
{
    (void)how;
    (void)set;
    (void)oldset;
    errno = ENOSYS;
    return -1;
}

int sigsuspend(const sigset_t *mask)
{
    (void)mask;
    errno = ENOSYS;
    return -1;
}

int kill(pid_t pid, int sig)
{
    (void)pid;
    (void)sig;
    errno = ENOSYS;
    return -1;
}

int killpg(int pgrp, int sig)
{
    (void)pgrp;
    (void)sig;
    errno = ENOSYS;
    return -1;
}

int raise(int sig)
{
    (void)sig;
    errno = ENOSYS;
    return -1;
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
    default: return (char *)"Unknown signal";
    }
}
