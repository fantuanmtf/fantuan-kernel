/* fantuan-kernel — interrupts.S (M1): the common ISR entry.
 * Saves the GP registers into the InterruptFrame layout, calls
 * isr_dispatch, restores the frame and iretq. The per-vector stubs live
 * in isr_stubs.S and jump here. Intel syntax; compiled via build.rs. */

    .intel_syntax noprefix

    .global isr_common

isr_common:
    /* Save all GP registers (the frame layout must match InterruptFrame). */
    push    rax
    push    rbx
    push    rcx
    push    rdx
    push    rsi
    push    rdi
    push    rbp
    push    r8
    push    r9
    push    r10
    push    r11
    push    r12
    push    r13
    push    r14
    push    r15

    mov     rdi, rsp        /* arg1 = *mut InterruptFrame */
    mov     rbp, rsp        /* remember the exact frame base */
    and     rsp, -16        /* SysV alignment for the call */
    sub     rsp, 128        /* reserve the ABI red zone for the handler */
    cld
    call    isr_dispatch

    mov     rsp, rbp
    pop     r15
    pop     r14
    pop     r13
    pop     r12
    pop     r11
    pop     r10
    pop     r9
    pop     r8
    pop     rbp
    pop     rdi
    pop     rsi
    pop     rdx
    pop     rcx
    pop     rbx
    pop     rax

    add     rsp, 16         /* vector + error code (or dummy) */
    iretq
