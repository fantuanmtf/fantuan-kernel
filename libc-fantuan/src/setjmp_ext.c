/* libc-fantuan — sigsetjmp/siglongjmp (P3): setjmp plus the signal mask.
 * The layout matches the glibc-style sigjmp_buf declared in setjmp.h.
 *
 * sigsetjmp is a macro there and expands at the call site, so the saved
 * jmp_buf belongs to the caller's frame; this file provides the pre-call
 * that captures the mask and the siglongjmp restore path. A plain
 * `sigsetjmp` function is still defined (with the macro undefined) for
 * callers that take its address; it is not the path compiled code uses. */
#include <setjmp.h>
#include <signal.h>

int __fantuan_sigsetjmp_pre(sigjmp_buf env, int savemask)
{
    if (savemask) {
        sigset_t set;
        if (sigprocmask(SIG_SETMASK, NULL, &set) == 0) {
            env[0].__mask = (unsigned long)set;
        }
        env[0].__savemask = 1;
    } else {
        env[0].__savemask = 0;
    }
    return 0;
}

void siglongjmp(sigjmp_buf env, int val)
{
    if (env[0].__savemask) {
        sigset_t set = (sigset_t)env[0].__mask;
        sigprocmask(SIG_SETMASK, &set, NULL);
    }
    longjmp(env[0].__jmp, val);
}

#undef sigsetjmp
int sigsetjmp(sigjmp_buf env, int savemask);
int sigsetjmp(sigjmp_buf env, int savemask)
{
    __fantuan_sigsetjmp_pre(env, savemask);
    return setjmp(env[0].__jmp);
}
