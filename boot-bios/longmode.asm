; fantuan-bios 64-bit payload stub (M10-2): loaded by stage2 at 0x10000 and
; entered in long mode with an identity map for the first 1 GiB. Prints the
; transition verdict over COM1 and halts. The real kernel entry replaces this
; stub in M10-3.

BITS 64
ORG 0x10000

longmode_entry:
    mov rsi, msg
    call puts64
    cli
.hang:
    hlt
    jmp .hang

; rsi -> NUL-terminated string
puts64:
    push rax
    push rdx
.loop:
    lodsb
    test al, al
    jz .done
    mov ah, al
.wait:
    mov dx, 0x3FD
    in al, dx
    test al, 0x20
    jz .wait
    mov dx, 0x3F8
    mov al, ah
    out dx, al
    jmp .loop
.done:
    pop rdx
    pop rax
    ret

msg: db 'long mode ok (M10-2): 64-bit stub running', 13, 10, 0

times 512-($-$$) db 0
