/* kernel-riscv link script (M9.1)
 * QEMU virt loads the ELF at 0x80200000; OpenSBI enters _start in S-mode
 * with a0 = hartid, a1 = DTB (verified by spike, DESIGN.md §14.1). */
ENTRY(_start)

SECTIONS
{
    . = 0x80200000;

    .text : ALIGN(4) {
        *(.text.entry)
        *(.text .text.*)
    }
    .rodata : ALIGN(8) {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }
    .data : ALIGN(8) {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }
    .bss : ALIGN(8) {
        __bss_start = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        *(COMMON)
        __bss_end = .;
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
    }
}
