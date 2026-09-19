; fantuan-bios stage1 (M10): a 512-byte MBR that initializes COM1, loads
; stage2 with int 0x13 LBA extensions into 0x0000:0x8000 and jumps to it.
; Built by tools/build-bios.sh (nasm), tested under QEMU SeaBIOS.
;
; Register/memory contract (spike-verified, see docs/M10_BOOT_32BIT.md):
;   - entered at 0x7C00 in real mode, dl = boot drive, cs:ip = 0:0x7C00
;   - DAP at 0x0600 (below the IVT/BDA safe area), stage2 at 0x8000
;   - stack top 0x7000 (grows down, clear of stage1/stage2/buffers)

BITS 16
ORG 0x7C00

%define STAGE2_SEG 0x0000
%define STAGE2_OFF 0x8000
%define STAGE2_LBA 1
%define STAGE2_SECTORS 32

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7000
    sti
    mov [boot_drive], dl

    call serial_init
    mov al, '1'
    call serial_putc

%ifndef CD_BOOT
    mov si, dap
    mov ah, 0x42
    mov dl, [boot_drive]
    int 0x13
    jc disk_error
%endif

    mov al, '2'
    call serial_putc

    mov dl, [boot_drive]
    jmp STAGE2_SEG:STAGE2_OFF

disk_error:
    mov al, 'E'
    call serial_putc
.hang:
    hlt
    jmp .hang

; --- COM1 (0x3F8), 115200 8N1 ------------------------------------------------

serial_init:
    mov dx, 0x3F9
    xor al, al
    out dx, al          ; no interrupts
    mov dx, 0x3FB
    mov al, 0x80
    out dx, al          ; DLAB on
    mov dx, 0x3F8
    mov al, 1
    out dx, al          ; divisor low (115200)
    mov dx, 0x3F9
    xor al, al
    out dx, al
    mov dx, 0x3FB
    mov al, 0x03
    out dx, al          ; 8N1
    mov dx, 0x3FA
    mov al, 0xC7
    out dx, al          ; FIFO enable + clear
    mov dx, 0x3FC
    mov al, 0x0B
    out dx, al          ; DTR + RTS
    ret

; al = byte to send
serial_putc:
    push ax
    push dx
    mov ah, al
.wait:
    mov dx, 0x3FD
    in al, dx
    test al, 0x20
    jz .wait
    mov dx, 0x3F8
    mov al, ah
    out dx, al
    pop dx
    pop ax
    ret

boot_drive: db 0

align 4
dap:
    db 0x10, 0          ; DAP size, reserved
    dw STAGE2_SECTORS
    dw STAGE2_OFF
    dw STAGE2_SEG
    dq STAGE2_LBA

times 510-($-$$) db 0
dw 0xAA55
