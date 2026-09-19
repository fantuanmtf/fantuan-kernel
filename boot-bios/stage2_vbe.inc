; fantuan-bios VBE setup (M10-5): real-mode INT 10h queries that pick one
; linear-framebuffer mode and copy the ROM 8x16 font into low memory. Called
; from stage2 before protected mode on the i686 build; the BootInfo build and
; the 32-bit page directory (stage2_pm.inc) consume the variables below. Any
; failure leaves every fb field zero and the kernel uses serial alone.
;
; INT 10h AX=4F00 (controller info), 4F01 (mode info), 4F02 with bit 14 (set
; the mode with the linear framebuffer), 1130 BH=6 (ROM 8x16 font pointer).
; The kernel accesses the LFB at 0xBFC00000, the PSE slot stage2 maps for the
; 4 MiB-aligned physical range.

%define VBE_INFO_ADDR 0x1000
%define VBE_MODE_ADDR 0x1200
; 0x90000 is below the 0x9FC00 top of usable low RAM, above the 0x80000
; kernel stack top, and clear of the CD preload at 0xC000..0x7C000.
%define VBE_FONT_ADDR 0x90000
%define VBE_FONT_BYTES 4096
%define VBE_CAND_COUNT 6

vbe_setup:
    mov dword [vbe_fb_phys], 0
    mov dword [vbe_pitch], 0
    mov word [vbe_width], 0
    mov word [vbe_height], 0
    mov byte [vbe_bpp], 0
    mov dword [vbe_font_phys], 0

    ; Controller info; prefill "VBE2" to request the 2.0 extended block.
    mov dword [VBE_INFO_ADDR], 'VBE2'
    push ds
    xor ax, ax
    mov es, ax
    mov di, VBE_INFO_ADDR
    mov ax, 0x4F00
    int 0x10
    pop ds
    cmp ax, 0x004F
    jne .done
    cmp word [VBE_INFO_ADDR + 4], 0x0200
    jb .done
    ; SeaBIOS answers "VESA" even for a VBE2 request, so accept both; the
    ; version check above is what guarantees the 2.0 fields exist.
    mov eax, [VBE_INFO_ADDR]
    cmp eax, 'VBE2'
    je .sig_ok
    cmp eax, 'VESA'
    jne .done
.sig_ok:
    mov byte [vbe_cand], 0
.cand:
    movzx bx, byte [vbe_cand]
    cmp bx, VBE_CAND_COUNT
    jae .done
    imul bx, bx, 6
    add bx, vbe_candidates
    mov ax, [bx]
    mov [vbe_req_w], ax
    mov ax, [bx + 2]
    mov [vbe_req_h], ax
    mov al, [bx + 4]
    mov [vbe_req_bpp], al
    mov ax, [VBE_INFO_ADDR + 0x0E]
    mov [vbe_mlist_off], ax
    mov ax, [VBE_INFO_ADDR + 0x10]
    mov [vbe_mlist_seg], ax
.mode:
    mov ax, [vbe_mlist_seg]
    mov es, ax
    mov si, [vbe_mlist_off]
    mov dx, [es:si]
    add si, 2
    mov [vbe_mlist_off], si
    cmp dx, 0xFFFF
    je .next
    mov [vbe_mode], dx
    mov cx, dx
    push ds
    xor ax, ax
    mov es, ax
    mov di, VBE_MODE_ADDR
    mov ax, 0x4F01
    int 0x10
    pop ds
    cmp ax, 0x004F
    jne .mode
    ; supported graphics mode with a linear framebuffer, one plane,
    ; packed-pixel or direct-color, and exactly the requested geometry
    mov ax, [VBE_MODE_ADDR + 0x00]
    test ax, 0x0010
    jz .mode
    test ax, 0x0080
    jz .mode
    cmp byte [VBE_MODE_ADDR + 0x18], 1
    jne .mode
    mov al, [VBE_MODE_ADDR + 0x1B]
    cmp al, 4
    je .model
    cmp al, 6
    jne .mode
.model:
    mov ax, [VBE_MODE_ADDR + 0x12]
    cmp ax, [vbe_req_w]
    jne .mode
    mov ax, [VBE_MODE_ADDR + 0x14]
    cmp ax, [vbe_req_h]
    jne .mode
    mov al, [VBE_MODE_ADDR + 0x19]
    cmp al, [vbe_req_bpp]
    jne .mode
    mov eax, [VBE_MODE_ADDR + 0x28]        ; PhysBasePtr
    test eax, eax
    jz .mode
    movzx ecx, word [VBE_MODE_ADDR + 0x10] ; pitch
    movzx eax, word [vbe_req_w]
    movzx edx, byte [vbe_req_bpp]
    imul eax, edx
    shr eax, 3
    cmp ecx, eax                           ; sane pitch: at least one line
    jb .mode
    movzx eax, word [vbe_req_h]
    imul eax, ecx                          ; pitch * height
    mov edx, [VBE_MODE_ADDR + 0x28]
    and edx, 0x3FFFFF                      ; offset into the 4 MiB PSE slot
    add eax, edx
    cmp eax, 0x400000                      ; must fit the single mapped slot
    ja .mode
    mov bx, [vbe_mode]
    or bx, 0x4000                          ; linear framebuffer
    push ds
    mov ax, 0x4F02
    int 0x10
    pop ds
    cmp ax, 0x004F
    jne .mode
    mov eax, [VBE_MODE_ADDR + 0x28]
    mov [vbe_fb_phys], eax
    movzx eax, word [VBE_MODE_ADDR + 0x10]
    mov [vbe_pitch], eax
    mov ax, [VBE_MODE_ADDR + 0x12]
    mov [vbe_width], ax
    mov ax, [VBE_MODE_ADDR + 0x14]
    mov [vbe_height], ax
    mov al, [VBE_MODE_ADDR + 0x19]
    mov [vbe_bpp], al
    jmp vbe_font_copy
.next:
    inc byte [vbe_cand]
    jmp .cand
.done:
    ret

; AX=1130 BH=6 returns ES:BP = ROM 8x16 font and CX = bytes per character;
; copy all 256 glyphs to VBE_FONT_ADDR and verify the 'A' glyph is nonzero.
vbe_font_copy:
    push ds
    push es
    mov ax, 0x1130
    mov bh, 6
    int 0x10
    cmp cx, 16
    jne .pop_ret
    mov si, bp
    mov dx, es
    mov ax, VBE_FONT_ADDR >> 4
    mov es, ax
    mov ds, dx
    xor di, di
    mov cx, VBE_FONT_BYTES / 2
    cld
    rep movsw
    mov si, 0x41 * 16
    mov cx, 16
    xor dl, dl
.check:
    mov al, [es:si]
    or dl, al
    inc si
    loop .check
    pop es
    pop ds
    test dl, dl
    jz .ret
    mov dword [vbe_font_phys], VBE_FONT_ADDR
.ret:
    ret
.pop_ret:
    pop es
    pop ds
    ret

; Most preferred first: 1024x768x32, then 800x600 (32/24), then 640x480
; (32/24/16). Every entry is checked against the mode list and rejected when
; its pitch or total size does not fit the 4 MiB slot.
vbe_candidates:
    dw 1024, 768, 32
    dw 800, 600, 32
    dw 800, 600, 24
    dw 640, 480, 32
    dw 640, 480, 24
    dw 640, 480, 16

vbe_fb_phys:   dd 0
vbe_pitch:     dd 0
vbe_width:     dw 0
vbe_height:    dw 0
vbe_bpp:       db 0
vbe_font_phys: dd 0
vbe_mode:      dw 0
vbe_req_w:     dw 0
vbe_req_h:     dw 0
vbe_req_bpp:   db 0
vbe_cand:      db 0
vbe_mlist_off: dw 0
vbe_mlist_seg: dw 0
