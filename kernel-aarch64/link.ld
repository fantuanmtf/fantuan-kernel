/* kernel-aarch64 link script (M11 R9a).
 * QEMU `virt` loads the ELF segments at their physical addresses and enters
 * _start at EL1 with x0 = DTB. 0x40080000 mirrors the arm64 Linux image
 * offset: it leaves the low RAM page for the DTB/firmware and stays well
 * below QEMU's DTB placement. */
ENTRY(_start)

SECTIONS
{
    . = 0x40080000;

    .text : ALIGN(4) {
        *(.text.entry)
        *(.text .text.*)
    }
    .rodata : ALIGN(8) {
        *(.rodata .rodata.*)
        /* NetBSD libkern-style link sets (kernel-net event counters), as in
         * the x86_64 script: the section name is not a C identifier, so the
         * __start/__stop symbols must be defined here. */
        __start_link_set_evcnts = .;
        KEEP(*(link_set_evcnts))
        __stop_link_set_evcnts = .;
    }
    .data : ALIGN(8) {
        *(.data .data.*)
    }
    .bss : ALIGN(8) {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
    }
}
