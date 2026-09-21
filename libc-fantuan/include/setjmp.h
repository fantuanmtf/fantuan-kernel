/* libc-fantuan — setjmp.h (P2): real x86_64 setjmp/longjmp (setjmp.S). */
#ifndef _SETJMP_H
#define _SETJMP_H

/* rbx, rbp, r12..r15, rsp, rip */
typedef unsigned long jmp_buf[8];

int setjmp(jmp_buf env);
void longjmp(jmp_buf env, int val) __attribute__((noreturn));
int _setjmp(jmp_buf env);
void _longjmp(jmp_buf env, int val) __attribute__((noreturn));

#endif /* _SETJMP_H */
