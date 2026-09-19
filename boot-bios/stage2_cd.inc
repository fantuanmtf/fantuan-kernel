; fantuan-bios stage2 CD half (M10-6, assembled with -D CD_BOOT=1 for the
; hybrid ISO's El Torito no-emulation entry).
;
; The BIOS boot image is laid out for the firmware preload at 0x7C00:
;   0x7C00  stage1 (512 B)       image offset 0x0000
;   0x7E00  alignment padding    image offset 0x0200
;   0x8000  stage2 (16 KiB)      image offset 0x0400
;   0xC000  kernel (KSIZE)       image offset 0x4400
; SeaBIOS and the spec load sector_count 512-byte sectors of the boot image
; at the load segment before jumping; the kernel is therefore already in low
; memory. cd_kernel_copy moves it to KERNEL_PHYS from protected mode, where
; the 16-bit DAP of the raw-disk path cannot reach. No INT 13h and no ATA
; access are used on the CD path.

BITS 32

%define CD_STAGE 0xC000

cd_kernel_copy:
    mov esi, CD_STAGE
    mov edi, KERNEL_PHYS
    mov ecx, (KERNEL_SECTORS * 512 + 3) / 4
    cld
    rep movsd
    ret

BITS 16
