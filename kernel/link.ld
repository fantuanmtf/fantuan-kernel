/* fantuan-kernel link script (M0)
 * Flat kernel linked at 16 MiB physical; the bootloader copies the binary there.
 * Low memory is a firmware minefield (the EFI app itself may sit above 1 MiB),
 * so the kernel lives well clear. .text.entry must come first so _start lands
 * exactly at 0x1000000. */
ENTRY(_start)

SECTIONS
{
    . = 0x1000000;

    .text : ALIGN(4K) {
        *(.text.entry)
        *(.text.*)
    }
    .rodata : ALIGN(4K) {
        *(.rodata.*)
    }
    .data : ALIGN(4K) {
        *(.data.*)
    }
    .bss : ALIGN(4K) {
        *(.bss.*)
        *(COMMON)
    }

    __bss_start = ADDR(.bss);
    __bss_end = ADDR(.bss) + SIZEOF(.bss);
}
