/* fantuan-kernel — switch.S (M4)
 * Context switch — the "forever assembly" #2 (DESIGN.md §2).
 *
 * SysV ABI: switch_context(old_rsp: *mut u64, new_rsp: u64, new_cr3: u64)
 *   rdi = &old_rsp   rsi = new_rsp   rdx = new_cr3 (physical PML4)
 * Saves the callee-saved registers on the current task's kernel stack, stores
 * rsp into *old_rsp, loads the next task's rsp and page tables, restores, and
 * returns — into the next task's saved call chain (or into task_entry /
 * user_entry for a fresh task). Kernel mappings are shared by every page
 * table, so the switch itself stays mapped throughout.
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
    mov     cr3, rdx

    pop     r15
    pop     r14
    pop     r13
    pop     r12
    pop     rbp
    pop     rbx
    ret

/* user_entry — first entry into a user task. switch_context returns here with
 * rsp already pointing at the pre-built ring-3 frame:
 *   [rip][cs][rflags][rsp][ss]
 * so a bare iretq lands in user mode. */
    .global user_entry
user_entry:
    iretq
