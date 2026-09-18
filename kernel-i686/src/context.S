/* kernel-i686 context switch (M10-4b2b). cdecl:
 *   context_switch(&old_rsp, new_rsp, new_cr3)
 * Saves ebp/ebx/esi/edi on the current stack, publishes the new esp,
 * switches CR3 when non-zero, restores from the incoming stack and returns
 * into the resumed context (kernel-core::task drives it). */

    .intel_syntax noprefix
    .section .text
    .global context_switch
context_switch:
    push ebp
    push ebx
    push esi
    push edi
    mov eax, [esp + 20]        /* &old_rsp (4 regs + ret + 3 args) */
    mov [eax], esp
    mov eax, [esp + 24]        /* new_rsp */
    mov ecx, [esp + 28]        /* new_cr3, 0 = keep */
    test ecx, ecx
    jz 1f
    mov cr3, ecx
1:
    mov esp, eax
    pop edi
    pop esi
    pop ebx
    pop ebp
    ret
