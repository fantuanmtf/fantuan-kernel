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

    mov si, msg_spike
    call serial_puts

    ; Load the 64-bit payload (LBA 9, up to 16 sectors) to 0x1000:0x0000.
    mov si, payload_dap
    mov ah, 0x42
    mov dl, [boot_drive]
    int 0x13
    jc load_error

    mov si, msg_long
    call serial_puts
    jmp enter_long

load_error:
    mov si, msg_load_err
    call serial_puts
.hang:
    hlt
    jmp .hang

; --- long mode transition (M10-2) ---------------------------------------------

enter_long:
    cli
    ; fast A20 gate
    in al, 0x92
    or al, 2
    out 0x92, al

    lgdt [gdt64_ptr]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:pm32

BITS 32
pm32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov esp, 0x7000

    ; page tables: PML4 0x1000, PDPT 0x2000, PD 0x3000 (identity, 1 GiB)
    xor eax, eax
    mov ecx, 0xC00
    mov edi, 0x1000
    rep stosd
    mov dword [0x1000], 0x2003
    mov dword [0x2000], 0x3003
    mov edi, 0x3000
    mov eax, 0x83
    mov ecx, 512
.fill:
    mov [edi], eax
    add edi, 8
    add eax, 0x200000
    loop .fill

    mov eax, cr4
    or eax, 1 << 5
    mov cr4, eax
    mov eax, 0x1000
    mov cr3, eax
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 8
    wrmsr
    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax
    jmp 0x18:long_entry

BITS 64
long_entry:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov rsp, 0x7000
    mov rax, 0x10000
    jmp rax

BITS 16

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
msg_long:   db 'entering long mode...', 13, 10, 0
msg_load_err: db 'E: payload load failed', 13, 10, 0

boot_drive: db 0

align 4
payload_dap:
    db 0x10, 0
    dw 8
    dw 0x0000
    dw 0x1000
    dq 9

align 8
gdt64:
    dq 0x0000000000000000
    dq 0x00CF9A000000FFFF ; 0x08: 32-bit code
    dq 0x00CF92000000FFFF ; 0x10: data
    dq 0x00AF9A000000FFFF ; 0x18: 64-bit code
gdt64_end:
gdt64_ptr:
    dw gdt64_end - gdt64 - 1
    dd gdt64

e820_count: dw 0
e820_index: dw 0

times 4096-($-$$) db 0
