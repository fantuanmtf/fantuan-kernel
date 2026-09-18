; fantuan-bios stage2 (M10 spike): prints the banner, collects the E820
; memory map into 0x9000 (max 32 entries), prints the count and the first
; entries, then halts. Long-mode entry and kernel load land in M10-2.
; Real-mode contract: dl = boot drive, ds = es = ss = 0, sp valid.

BITS 16
ORG 0x8000

%define E820_ADDR 0x9000
%define E820_MAX 32
%define E820_MAGIC 0x534D4150 ; 'SMAP'

start:
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

    mov si, msg_spike
    call serial_puts
.hang:
    hlt
    jmp .hang

; --- E820 ---------------------------------------------------------------------

e820_collect:
    mov word [e820_count], 0
    mov di, E820_ADDR
    xor ebx, ebx
    mov ecx, 24
.next:
    mov eax, 0xE820
    mov edx, E820_MAGIC
    ; es:di already set, ecx = entry size
    int 0x15
    jc .done
    cmp eax, E820_MAGIC
    jne .done
    ; store the entry (the final one reports ebx == 0 and is still valid)
    inc word [e820_count]
    cmp word [e820_count], E820_MAX
    jae .done
    test ebx, ebx
    jz .done
    add di, 24
    mov ecx, 24
    jmp .next
.done:
    ret

; Print base[31:0], length[31:0] and type for every collected entry.
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
    add di, 24
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

; eax = 32-bit value -> 8 hex digits
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

; ax -> decimal
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

msg_banner: db 'fantuan-bios stage2 (M10 spike)', 13, 10, 0
msg_count:  db 'e820: entries=', 0
msg_index:  db 'e820[', 0
msg_colon:  db ']:', 0
msg_base:   db ' base=', 0
msg_len:    db ' len=', 0
msg_type:   db ' type=', 0
msg_crlf:   db 13, 10, 0
msg_spike:  db 'spike: stage1+stage2+LBA read+E820 ok', 13, 10, 0

e820_count: dw 0
e820_index: dw 0

times 4096-($-$$) db 0
