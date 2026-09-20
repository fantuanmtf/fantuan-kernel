/* libc-fantuan — crt0.S (P1): process entry.
 *
 * The kernel passes the SysV stack `[argc][argv...][NULL][envp NULL]` with
 * RSP 16-byte aligned (kernel/src/task/mod.rs build_user_stack). We hand
 * argc/argv/envp to __libc_start_main, which runs main and exits. */
    .text
    .global _start
    .type _start, @function
_start:
    xorq %rbp, %rbp              /* outermost frame */
    movq (%rsp), %rdi            /* argc */
    leaq 8(%rsp), %rsi           /* argv */
    leaq 8(%rsi,%rdi,8), %rdx    /* envp = argv + argc + 1 */
    andq $-16, %rsp              /* keep the call ABI aligned */
    call __libc_start_main
    hlt                          /* not reached */
    .size _start, .-_start

/* Non-executable stack marker (linker hygiene). */
    .section .note.GNU-stack,"",@progbits
