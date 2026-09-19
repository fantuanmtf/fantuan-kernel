; fantuan-bios stage2 (M10-3): real-mode half.
;   banner -> E820 collect/dump -> GDT -> protected mode (stage2_pm.inc does
;   the BootInfo synthesis, ATA kernel load, page tables and the jump).
; BootInfo constants shared with stage2_pm.inc; mdBook offsets:
;   BOOTINFO_PHYS 0x4000, MEMMAP_PHYS 0x5000 (24 x 40 B descriptors)
;   E820 buffer 0x9000 (24 B entries), stack top 0x80000, tables 0x20000.
; The lowest 1 MiB is reported reserved so the kernel never reuses any of it.

BITS 16
ORG 0x8000

%define E820_ADDR 0x9000
%define E820_MAX 32
%define E820_MAGIC 0x534D4150
%define E820_ENTRY 24

%define BOOTINFO_PHYS 0x4000
%define MEMMAP_PHYS 0x5000
%define MEMMAP_MAX 24
%define KERNEL_PHYS 0x1000000
%define STACK_TOP 0x80000
%define PML4_PHYS 0x20000
%ifdef I686
%define KERNEL_ENTRY 0xC1000000
%else
%define KERNEL_ENTRY 0xFFFF800001000000
%endif

start:
    mov [boot_drive], dl
    mov si, msg_banner
    call serial_puts

    call e820_collect
    mov si, msg_count
    call serial_puts
    mov ax, [e820_count]
    call put_dec16
    mov si, msg_crlf
    call serial_puts
    call e820_print_all

%ifdef I686
    call vbe_setup
%endif

    mov si, msg_long
    call serial_puts

    cli
    in al, 0x92
    or al, 2
    out 0x92, al
    lgdt [gdt_ptr]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:pm32

; --- E820 ---------------------------------------------------------------------

e820_collect:
    mov word [e820_count], 0
    mov di, E820_ADDR
    xor ebx, ebx
    mov ecx, E820_ENTRY
.next:
    mov eax, 0xE820
    mov edx, E820_MAGIC
    int 0x15
    jc .done
    cmp eax, E820_MAGIC
    jne .done
    inc word [e820_count]
    cmp word [e820_count], E820_MAX
    jae .done
    test ebx, ebx
    jz .done
    add di, E820_ENTRY
    mov ecx, E820_ENTRY
    jmp .next
.done:
    ret

e820_print_all:
    mov cx, [e820_count]
    test cx, cx
    jz .ret
    mov di, E820_ADDR
    mov word [e820_index], 0
.loop:
    push cx
    mov si, msg_index
    call serial_puts
    mov ax, [e820_index]
    call put_dec16
    mov si, msg_colon
    call serial_puts
    call e820_print_entry
    pop cx
    add di, E820_ENTRY
    inc word [e820_index]
    loop .loop
.ret:
    ret

; di -> entry
e820_print_entry:
    mov si, msg_base
    call serial_puts
    mov eax, [di]
    call put_hex32
    mov si, msg_len
    call serial_puts
    mov eax, [di + 8]
    call put_hex32
    mov si, msg_type
    call serial_puts
    mov eax, [di + 16]
    call put_hex32
    mov si, msg_crlf
    call serial_puts
    ret

; --- output helpers -----------------------------------------------------------

serial_puts:
    push ax
    push si
.loop:
    lodsb
    test al, al
    jz .done
    call serial_putc
    jmp .loop
.done:
    pop si
    pop ax
    ret

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

put_hex32:
    push ax
    push cx
    push dx
    mov cx, 8
.loop:
    rol eax, 4
    push eax
    and al, 0x0F
    cmp al, 10
    jb .digit
    add al, 'a' - 10
    jmp .emit
.digit:
    add al, '0'
.emit:
    call serial_putc
    pop eax
    loop .loop
    pop dx
    pop cx
    pop ax
    ret

put_dec16:
    push ax
    push bx
    push cx
    push dx
    mov cx, 0
    mov bx, 10
.loop:
    xor dx, dx
    div bx
    push dx
    inc cx
    test ax, ax
    jnz .loop
.emit:
    pop ax
    add al, '0'
    call serial_putc
    loop .emit
    pop dx
    pop cx
    pop bx
    pop ax
    ret

; --- data ---------------------------------------------------------------------

msg_banner: db 'fantuan-bios stage2 (M10-3)', 13, 10, 0
msg_count:  db 'e820: entries=', 0
msg_index:  db 'e820[', 0
msg_colon:  db ']:', 0
msg_base:   db ' base=', 0
msg_len:    db ' len=', 0
msg_type:   db ' type=', 0
msg_crlf:   db 13, 10, 0
msg_long:   db 'entering protected mode...', 13, 10, 0

boot_drive: db 0
e820_count: dw 0
e820_index: dw 0
mm_count: dw 0

align 8
gdt:
    dq 0x0000000000000000 ; null
    dq 0x00CF9A000000FFFF ; 0x08: 32-bit code
    dq 0x00CF92000000FFFF ; 0x10: data
    dq 0x00AF9A000000FFFF ; 0x18: 64-bit code
gdt_end:
gdt_ptr:
    dw gdt_end - gdt - 1
    dd gdt

%include "stage2_vbe.inc"
%include "stage2_pm.inc"

%ifdef CD_BOOT
%include "stage2_cd.inc"
%endif

times 16384-($-$$) db 0
