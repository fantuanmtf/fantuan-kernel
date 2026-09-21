/* libc-fantuan — syscall.S (P1): SysV args -> the fantuan register ABI.
 *
 * int 0x60: rax = number, rdi/rsi/rdx/r10/r8 = five args, result in rax.
 * The CPU switches to the kernel stack (TSS.rsp0), so the user red zone is
 * never touched and no scratch reservation is needed. */
    .text
    .global __fantuan_syscall6
    .type __fantuan_syscall6, @function
__fantuan_syscall6:
    movq %rdi, %rax
    movq %rsi, %rdi
    movq %rdx, %rsi
    movq %rcx, %rdx
    movq %r8, %r10
    movq %r9, %r8
    int $0x60
    ret
    .size __fantuan_syscall6, .-__fantuan_syscall6

/* P2 signal restorer: entered by the handler's `ret`, with RSP pointing at
 * the SignalFrame the kernel pushed. No stack use before SYS_SIGRETURN so
 * the kernel reads the frame at exactly this RSP. */
    .global __fantuan_sigreturn_trampoline
    .type __fantuan_sigreturn_trampoline, @function
__fantuan_sigreturn_trampoline:
    movq $35, %rax
    xorl %edi, %edi
    xorl %esi, %esi
    xorl %edx, %edx
    xorl %r10d, %r10d
    xorl %r8d, %r8d
    int $0x60
    hlt
    .size __fantuan_sigreturn_trampoline, .-__fantuan_sigreturn_trampoline

/* Non-executable stack marker (linker hygiene). */
    .section .note.GNU-stack,"",@progbits
