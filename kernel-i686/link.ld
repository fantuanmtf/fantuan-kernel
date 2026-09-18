/* kernel-i686 link script (M10): 3G/1G split; the kernel is linked at
 * PHYS_OFFSET + 16 MiB (0xC1000000), matching the physical load address. */
ENTRY(_start)

SECTIONS
{
    . = 0xC1000000;

    .text : ALIGN(4K) {
        *(.text.entry)
        *(.text.start)
        *(.text.*)
    }
    .rodata : ALIGN(4K) {
        *(.rodata.*)
    }
    .data : ALIGN(4K) {
        *(.data.*)
    }
    .bss : ALIGN(4K) {
        __bss_start = .;
        *(.bss.*)
        *(COMMON)
        __bss_end = .;
    }
}
