/* libc-fantuan — setjmp/longjmp (P2): x86_64 callee-saved context.
 *
 * jmp_buf layout (8 x 8 bytes): rbx, rbp, r12, r13, r14, r15, rsp, rip.
 * setjmp returns 0; longjmp restores and returns VAL (1 when VAL == 0). */
    .text

    .global setjmp
    .type setjmp, @function
setjmp:
    movq %rbx, 0(%rdi)
    movq %rbp, 8(%rdi)
    movq %r12, 16(%rdi)
    movq %r13, 24(%rdi)
    movq %r14, 32(%rdi)
    movq %r15, 40(%rdi)
    leaq 8(%rsp), %rax
    movq %rax, 48(%rdi)
    movq (%rsp), %rax
    movq %rax, 56(%rdi)
    xorl %eax, %eax
    ret
    .size setjmp, .-setjmp

    .global _setjmp
    .set _setjmp, setjmp

    .global longjmp
    .type longjmp, @function
longjmp:
    movq 0(%rdi), %rbx
    movq 8(%rdi), %rbp
    movq 16(%rdi), %r12
    movq 24(%rdi), %r13
    movq 32(%rdi), %r14
    movq 40(%rdi), %r15
    movq 48(%rdi), %rsp
    movq %rsi, %rax
    testq %rax, %rax
    jnz 1f
    movq $1, %rax
1:
    jmp *56(%rdi)
    .size longjmp, .-longjmp

    .global _longjmp
    .set _longjmp, longjmp

/* Non-executable stack marker (linker hygiene). */
    .section .note.GNU-stack,"",@progbits
