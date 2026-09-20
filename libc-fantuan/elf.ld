/* libc-fantuan link script (P1): static ET_EXEC at 0x400000, matching the
 * kernel ELF loader (ET_EXEC + PT_LOAD only; user stack at 0x304000, heap
 * base 0x500000 from abi). Keeps the same layout as user/link.ld so both
 * Rust and C programs load through the one path. */
ENTRY(_start)

SECTIONS
{
    . = 0x400000;

    .text : ALIGN(4K) {
        *(.text._start)
        *(.text .text.*)
    }
    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
    }
    .data : ALIGN(4K) {
        *(.data .data.*)
    }
    .bss : ALIGN(4K) {
        *(.bss .bss.*)
        *(COMMON)
    }
}
