/* Ring-3 entry trampoline (M10-4b3). The scheduler's context_switch pops
 * edi/esi/ebx/ebp and returns here; esp points at the iret frame
 * [eip][cs][eflags][esp][ss]. Load the user data segments and iretd. */
    .intel_syntax noprefix
    .section .text
    .global user_entry
user_entry:
    mov ax, 0x23
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    iretd

/* gdt_flush(&gdtp): load the GDT and reload CS (far jump) + data segments. */
    .global gdt_flush
gdt_flush:
    mov eax, [esp + 4]
    lgdt [eax]
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    .byte 0xEA                     /* ljmp 0x08:<absolute offset> */
    .long gdt_flush_done
    .word 0x08
gdt_flush_done:
    ret
