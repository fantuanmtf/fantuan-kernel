/* fantuan-kernel link script (M3)
 * Flat kernel linked at PHYS_OFFSET + 16 MiB (0xFFFF800001000000); the
 * bootloader loads the binary at physical 16 MiB, builds initial page tables
 * (identity 4 GiB + PHYS_OFFSET alias) and jumps to the higher-half entry.
 * The base must match fantuan_abi::PHYS_OFFSET + 0x1000000 — kmain asserts it.
 * .text.entry must come first so _start lands at the link address.
 *
 * The kernel uses -C code-model=large, so LLVM emits the l-prefixed section
 * names (.ltext/.lrodata/.ldata/.lbss): BOTH families must be matched, or the
 * statics get orphan-placed outside __bss_start..__bss_end and are never
 * zeroed (a real bug we hit in M3). */
ENTRY(_start)

SECTIONS
{
    . = 0xFFFF800001000000;

    .text : ALIGN(4K) {
        *(.text.entry)
        *(.text.*)
        *(.ltext)
        *(.ltext.*)
    }
    .rodata : ALIGN(4K) {
        *(.rodata.*)
        *(.lrodata)
        *(.lrodata.*)
    }
    .data : ALIGN(4K) {
        *(.data.*)
        *(.ldata)
        *(.ldata.*)
    }
    .bss : ALIGN(4K) {
        *(.bss.*)
        *(.lbss)
        *(.lbss.*)
        *(COMMON)
    }

    __bss_start = ADDR(.bss);
    __bss_end = ADDR(.bss) + SIZEOF(.bss);
}
