; Ring-3 fault regression (M10-4b3b), assembled flat by build.rs and mapped
; at USER_BASE by user.rs. #UD from ring 3 must kill the task, not the kernel.
    bits 32
    org 0x400000

    ud2
.hang:
    jmp .hang
