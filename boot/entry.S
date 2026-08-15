/* fantuan-kernel — boot/entry.S (M1)
 * The "forever assembly": kernel entry stub after the UEFI handoff.
 *
 * Entry state (set by fantuan-boot):
 *   rdi = &BootInfo
 *   rsi = stack_top
 *   long mode, identity-mapped, interrupt state unknown -> we cli first.
 *
 * Stage-0 GDT for hygiene only; gdt.rs installs the authoritative GDT (with
 * the TSS descriptor) during kmain. All magic numbers come from
 * kernel/src/consts.rs via the generated asm_defs.inc — raw hex is forbidden
 * (DESIGN.md §13.3). Compiled by cc via kernel/build.rs. */

    .intel_syntax noprefix
#include "asm_defs.inc"

    .section .text.entry
    .global _start
_start:
    cli
    mov     rsp, rsi

    /* Load our own stage-0 GDT; the firmware's may vanish under us later. */
    lgdt    [rip + gdtr]

    /* Reload CS via the far-return trick (robust across assemblers). */
    lea     rax, [rip + .Lreload_cs]
    push    KERNEL_CS
    push    rax
    retfq
.Lreload_cs:
    mov     ax, KERNEL_DS
    mov     ds, ax
    mov     es, ax
    mov     ss, ax
    mov     fs, ax
    mov     gs, ax

    /* Zero .bss: objcopy -O binary drops NOBITS sections.
       rep stosb needs rdi, so stash &BootInfo in r10 first and restore it
       before the call — rdi is the SysV first argument. */
    mov     r10, rdi
    lea     rdi, [rip + __bss_start]
    lea     rcx, [rip + __bss_end]
    sub     rcx, rdi
    xor     eax, eax
    rep stosb

    /* SysV ABI: 16-byte stack alignment at the call site. */
    and     rsp, -16
    mov     rdi, r10       /* restore &BootInfo */
    call    kmain

.Lhalt:
    hlt
    jmp     .Lhalt

    .align  8
gdt:
    .quad   0x0000000000000000      /* null */
    .quad   GDT_CODE64              /* 64-bit code segment */
    .quad   GDT_DATA64              /* 64-bit data segment */
gdtr:
    .word   . - gdt - 1
    .quad   gdt
