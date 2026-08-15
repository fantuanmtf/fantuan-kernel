/* fantuan-kernel — switch.S (M3)
 * Context switch — the "forever assembly" #2 (DESIGN.md §2).
 *
 * SysV ABI: switch_context(old_rsp: *mut u64, new_rsp: u64)
 *   rdi = &old_rsp   rsi = new_rsp
 * Saves the callee-saved registers on the current task's kernel stack, stores
 * rsp into *old_rsp, loads the next task's rsp, restores, and returns — into
 * the next task's saved call chain (or into task_entry for a fresh task).
 * Intel syntax; compiled by cc via kernel/build.rs. */

    .intel_syntax noprefix

    .global switch_context
switch_context:
    push    rbx
    push    rbp
    push    r12
    push    r13
    push    r14
    push    r15

    mov     [rdi], rsp
    mov     rsp, rsi

    pop     r15
    pop     r14
    pop     r13
    pop     r12
    pop     rbp
    pop     rbx
    ret
