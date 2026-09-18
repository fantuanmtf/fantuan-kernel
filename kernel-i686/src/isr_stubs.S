/* kernel-i686 ISR stubs (M10-4b2): 48 vectors (0..31 CPU exceptions,
 * 32..47 remapped PIC IRQs). Entry frame per vector:
 *   [vector][error-or-dummy][eip][cs][eflags]
 * After the C call the stub drops the vector plus the dummy/error word (8
 * bytes) and irets. Generated once; keep the error-vector set in sync with
 * idt.rs (the 48-entry table matches this file's stubs). */

    .intel_syntax noprefix
    .section .text

    .macro ISR_OK n
    .global isr_\n
isr_\n:
    push 0
    push \n
    jmp isr_common
    .endm

    .macro ISR_ERR n
    .global isr_\n
isr_\n:
    push \n
    jmp isr_common
    .endm

    ISR_OK 0
    ISR_OK 1
    ISR_OK 2
    ISR_OK 3
    ISR_OK 4
    ISR_OK 5
    ISR_OK 6
    ISR_OK 7
    ISR_ERR 8
    ISR_OK 9
    ISR_ERR 10
    ISR_ERR 11
    ISR_ERR 12
    ISR_ERR 13
    ISR_ERR 14
    ISR_OK 15
    ISR_OK 16
    ISR_ERR 17
    ISR_OK 18
    ISR_OK 19
    ISR_OK 20
    ISR_ERR 21
    ISR_OK 22
    ISR_OK 23
    ISR_OK 24
    ISR_OK 25
    ISR_OK 26
    ISR_OK 27
    ISR_OK 28
    ISR_ERR 29
    ISR_ERR 30
    ISR_OK 31
    ISR_OK 32
    ISR_OK 33
    ISR_OK 34
    ISR_OK 35
    ISR_OK 36
    ISR_OK 37
    ISR_OK 38
    ISR_OK 39
    ISR_OK 40
    ISR_OK 41
    ISR_OK 42
    ISR_OK 43
    ISR_OK 44
    ISR_OK 45
    ISR_OK 46
    ISR_OK 47

isr_common:
    pushad
    mov eax, [esp + 32]
    mov edx, [esp + 36]
    push edx
    push eax
    call isr_dispatch
    add esp, 8            /* drop the two dispatch arguments        */
    popad
    add esp, 8            /* drop vector + error/dummy, then iretd  */
    iretd

    .section .rodata
    .align 4
    .global isr_table
isr_table:
    .long isr_0
    .long isr_1
    .long isr_2
    .long isr_3
    .long isr_4
    .long isr_5
    .long isr_6
    .long isr_7
    .long isr_8
    .long isr_9
    .long isr_10
    .long isr_11
    .long isr_12
    .long isr_13
    .long isr_14
    .long isr_15
    .long isr_16
    .long isr_17
    .long isr_18
    .long isr_19
    .long isr_20
    .long isr_21
    .long isr_22
    .long isr_23
    .long isr_24
    .long isr_25
    .long isr_26
    .long isr_27
    .long isr_28
    .long isr_29
    .long isr_30
    .long isr_31
    .long isr_32
    .long isr_33
    .long isr_34
    .long isr_35
    .long isr_36
    .long isr_37
    .long isr_38
    .long isr_39
    .long isr_40
    .long isr_41
    .long isr_42
    .long isr_43
    .long isr_44
    .long isr_45
    .long isr_46
    .long isr_47

