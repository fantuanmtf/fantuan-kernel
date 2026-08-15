/* fantuan-kernel link script (M2)
 * Flat kernel linked at PHYS_OFFSET + 16 MiB (0xFFFF800001000000); the
 * bootloader loads the binary at physical 16 MiB, builds initial page tables
 * (identity 4 GiB + PHYS_OFFSET alias) and jumps to the higher-half entry.
 * The base must match fantuan_abi::PHYS_OFFSET + 0x1000000 — kmain asserts it.
 * .text.entry must come first so _start lands at the link address. */
ENTRY(_start)

SECTIONS
{
    . = 0xFFFF800001000000;

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
