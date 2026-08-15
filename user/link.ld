/* fantuan-user link script (M4)
 * Static ELF linked at 0x400000; the kernel's ELF loader maps the PT_LOAD
 * segments from this layout. Matches both default and l-prefixed section
 * names (the workspace uses -C code-model=large). */
ENTRY(_start)

SECTIONS
{
    . = 0x400000;

    .text : ALIGN(4K) {
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
}
