; Minimal ring-3 test program (M10-4b3), assembled to a flat binary by
; build.rs and loaded at USER_BASE. Syscall convention (int 0x80, Linux-like):
;   eax = number, args in ebx, ecx, edx (SYS_WRITE is (buf, len) per
;   fantuan-abi, not the Linux fd form).
    bits 32
    org 0x400000

SYS_EXIT  equ 1
SYS_WRITE equ 3

start:
    mov eax, SYS_WRITE
    mov ebx, msg                ; buf
    mov ecx, msg_len            ; len
    int 0x80

    mov eax, SYS_EXIT
    xor ebx, ebx
    int 0x80

.hang:
    jmp .hang

msg: db "user(i686): hello from ring 3", 10
msg_len equ $ - msg
