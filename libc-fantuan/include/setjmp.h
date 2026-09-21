/* libc-fantuan — setjmp.h (P2): real x86_64 setjmp/longjmp (setjmp.S). */
#ifndef _SETJMP_H
#define _SETJMP_H

/* rbx, rbp, r12..r15, rsp, rip */
typedef unsigned long jmp_buf[8];

/* P3: sigsetjmp adds the signal mask (glibc-compatible layout, saved by
 * src/setjmp_ext.c). sigjmp_buf is an array so it decays like jmp_buf. */
typedef struct {
    jmp_buf __jmp;
    unsigned long __mask;
    int __savemask;
} sigjmp_buf[1];

int setjmp(jmp_buf env) __attribute__((returns_twice));
void longjmp(jmp_buf env, int val) __attribute__((noreturn));
int _setjmp(jmp_buf env) __attribute__((returns_twice));
void _longjmp(jmp_buf env, int val) __attribute__((noreturn));
void siglongjmp(sigjmp_buf env, int val) __attribute__((noreturn));
int __fantuan_sigsetjmp_pre(sigjmp_buf env, int savemask);

/* sigsetjmp MUST expand at the call site: the setjmp context has to belong
 * to the caller's frame (a setjmp nested in another C function does not
 * restore reliably). The pre-call saves the mask; the setjmp itself is the
 * caller's. src/setjmp_ext.c also defines the plain function for link
 * completeness, but compiled callers get this macro. */
#define sigsetjmp(env, savemask) \
    (__fantuan_sigsetjmp_pre((env), (savemask)), setjmp((env)[0].__jmp))

#endif /* _SETJMP_H */
