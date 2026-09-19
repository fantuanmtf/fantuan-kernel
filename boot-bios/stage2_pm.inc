; fantuan-bios stage2 protected-mode half (M10-3): E820 -> BootInfo memmap,
; BootInfo struct, identity+alias page tables (4 GiB), ATA PIO kernel load,
; long-mode entry and the jump to the kernel. Included by stage2.asm so it
; shares its constants and data labels.

BITS 32

%ifndef KERNEL_LBA
%define KERNEL_LBA 33
%endif
%ifndef KERNEL_SECTORS
%define KERNEL_SECTORS 1
%endif
; The i686 smoke boots the kernel image as the primary slave so the test disk
; can be the primary master (the PIO ATA driver's target); the x86_64 BIOS
; path keeps the primary master. tools/build-bios.sh --slave sets this.
%ifndef KERNEL_DRIVE_SLAVE
%define KERNEL_DRIVE_SLAVE 0
%endif

%ifdef I686
%define BOOT_TABLES_PAGES 1
%else
%define BOOT_TABLES_PAGES 13
%endif

pm32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov esp, 0x7000

    call memmap_synth
    call bootinfo_build
%ifdef CD_BOOT
    call cd_kernel_copy
%endif
%ifdef I686
    call tables32_build
%else
    call tables_build
%endif
%ifndef CD_BOOT
    call ata_load
%endif

    mov esi, msg_boot
    call puts32
    mov esi, msg_mm
    call puts32
    movzx eax, word [mm_count]
    call put_dec32
    mov esi, msg_crlf
    call puts32

%ifdef I686
    ; 32-bit path: PSE 4 MiB pages, then a cdecl call into the kernel at
    ; its alias link address (the return address is a halt loop, unused).
    mov al, 'P'
    call putc32
    mov eax, cr4
    or eax, 0x10
    mov cr4, eax
    mov eax, PML4_PHYS
    mov cr3, eax
    mov eax, cr0
    or eax, 0x80000001
    mov cr0, eax
    mov al, 'J'
    call putc32
    mov esp, STACK_TOP
    push dword BOOTINFO_PHYS
    mov al, 'K'
    call putc32
    call KERNEL_ENTRY
    mov al, 'R'
    call putc32
    hlt
    jmp $
%else
    mov eax, cr4
    or eax, 1 << 5
    mov cr4, eax
    mov eax, PML4_PHYS
    mov cr3, eax
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 8
    wrmsr
    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax
    jmp 0x18:long64
%endif

; --- BootInfo memmap synthesis -------------------------------------------------

memmap_synth:
    mov word [mm_count], 0
    mov esi, E820_ADDR
    movzx ecx, word [e820_count]
    test ecx, ecx
    jz .done
    mov edi, MEMMAP_PHYS
.entry:
    mov eax, [esi + 8]
    or eax, [esi + 12]
    jz .next                     ; skip zero-length (SeaBIOS sentinel)
    mov eax, [esi]               ; base (lo)
    mov edx, [esi + 8]           ; length (lo)
    mov ebx, 0                   ; default: reserved
    cmp dword [esi + 16], 1
    jne .store
    cmp dword [esi + 4], 0
    jne .store                   ; above 4 GiB: report reserved in v1
    cmp dword [esi + 12], 0
    jne .store
    cmp eax, 0x100000
    jae .usable
    add edx, eax                 ; end = base + length
    jc .reload
    cmp edx, 0x100000
    jbe .next                    ; entirely inside the reserved low MiB
    sub edx, 0x100000
    mov eax, 0x100000
.usable:
    mov ebx, 7                   ; MEMORY_TYPE_CONVENTIONAL
    jmp .store
.reload:
    mov eax, [esi]
    mov edx, [esi + 8]
.store:
    mov [edi + 0], ebx
    mov dword [edi + 4], 0
    mov [edi + 8], eax
    mov dword [edi + 12], 0
    mov dword [edi + 16], 0
    mov dword [edi + 20], 0
    shr edx, 12
    mov [edi + 24], edx
    mov dword [edi + 28], 0
    mov dword [edi + 32], 0
    mov dword [edi + 36], 0
    add edi, 40
    inc word [mm_count]
    cmp word [mm_count], MEMMAP_MAX
    jae .done
.next:
    add esi, E820_ENTRY
    dec ecx
    jnz .entry
.done:
    ret

; --- BootInfo (repr(C); 64-bit size 152, i686 size 160; see abi/src/lib.rs) ---

bootinfo_build:
%ifdef I686
    ; 32-bit BootInfo: pointer-size usize fields and 4-byte u64 alignment.
    xor eax, eax
    mov ecx, 160 / 4
    mov edi, BOOTINFO_PHYS
    rep stosd
    mov edi, BOOTINFO_PHYS
    mov dword [edi + 0], 0x46544E46      ; BOOT_MAGIC
    mov dword [edi + 4], 2               ; BOOT_VERSION
    mov dword [edi + 8], MEMMAP_PHYS     ; memmap.ptr
    movzx eax, word [mm_count]
    mov [edi + 12], eax                  ; memmap.count
    mov dword [edi + 16], 40             ; memmap.desc_size
    mov dword [edi + 60], KERNEL_PHYS    ; kernel_base
    mov dword [edi + 68], STACK_TOP      ; stack_top
    mov dword [edi + 84], PML4_PHYS      ; boot_pml4
    mov dword [edi + 92], BOOT_TABLES_PAGES
    mov dword [edi + 116], 3             ; arch = 3 (BIOS)
    ; M10-5 framebuffer fields (32-bit BootInfo offsets 136..160)
    mov eax, [vbe_fb_phys]
    mov [edi + 136], eax                 ; fb_phys (low dword; high is 0)
    mov eax, [vbe_pitch]
    mov [edi + 144], eax                 ; fb_pitch
    movzx eax, word [vbe_width]
    mov [edi + 148], ax                  ; fb_width
    movzx eax, word [vbe_height]
    mov [edi + 150], ax                  ; fb_height
    mov al, [vbe_bpp]
    mov [edi + 152], al                  ; fb_bpp
    mov eax, [vbe_font_phys]
    mov [edi + 156], eax                 ; fb_font_phys
%else
    xor eax, eax
    mov ecx, 152 / 4
    mov edi, BOOTINFO_PHYS
    rep stosd
    mov edi, BOOTINFO_PHYS
    mov dword [edi + 0], 0x46544E46      ; BOOT_MAGIC
    mov dword [edi + 4], 2               ; BOOT_VERSION
    mov dword [edi + 8], MEMMAP_PHYS     ; memmap.ptr
    movzx eax, word [mm_count]
    mov [edi + 16], eax                  ; memmap.count
    mov dword [edi + 24], 40             ; memmap.desc_size
    mov dword [edi + 72], KERNEL_PHYS    ; kernel_base
    mov dword [edi + 80], STACK_TOP      ; stack_top
    mov dword [edi + 96], PML4_PHYS      ; boot_pml4
    mov dword [edi + 104], BOOT_TABLES_PAGES
    mov dword [edi + 128], 3             ; arch = 3 (BIOS)
%endif
    ret

; --- page tables: identity + alias for 4 GiB, 2 MiB pages ----------------------

tables_build:
    xor eax, eax
    mov ecx, BOOT_TABLES_PAGES * 1024
    mov edi, PML4_PHYS
    rep stosd
    xor ebx, ebx
.gib:
    mov eax, ebx
    shl eax, 12
    lea edx, [eax + 0x21000]             ; identity PDPT page
    or edx, 3
    mov [PML4_PHYS + ebx * 8], edx
    lea edx, [eax + 0x25000]             ; alias PDPT page
    or edx, 3
    mov [PML4_PHYS + 256 * 8 + ebx * 8], edx
    lea edx, [eax + 0x29000]             ; shared PD page
    or edx, 3
    mov [0x21000 + ebx * 8], edx
    mov [0x25000 + ebx * 8], edx
    mov edi, 0x29000
    add edi, eax
    mov ecx, 512
    mov eax, ebx
    shl eax, 30
    or eax, 0x83
.fill:
    mov [edi], eax
    add edi, 8
    add eax, 0x200000
    loop .fill
    inc ebx
    cmp ebx, 4
    jb .gib
    ret

; --- 32-bit page directory (i686): identity + alias, 4 MiB PSE pages ----------

tables32_build:
    xor eax, eax
    mov ecx, 1024
    mov edi, PML4_PHYS
    rep stosd
    xor ebx, ebx
.ident:
    mov eax, ebx
    shl eax, 22
    or eax, 0x83
    mov [PML4_PHYS + ebx * 4], eax
    inc ebx
    cmp ebx, 256
    jb .ident
    xor ebx, ebx
.alias:
    mov eax, ebx
    shl eax, 22
    or eax, 0x83
    mov [PML4_PHYS + 768 * 4 + ebx * 4], eax
    inc ebx
    cmp ebx, 256
    jb .alias
    ; M10-5: PDE 767 (0xBFC00000) maps the 4 MiB-aligned VBE LFB range in
    ; every address space; the kernel and the per-task clones reach the
    ; console there instead of through the 1 GiB-bounded PHYS_OFFSET alias.
    mov eax, [vbe_fb_phys]
    test eax, eax
    jz .no_fb
    and eax, 0xFFC00000
    or eax, 0x83
    mov [PML4_PHYS + 767 * 4], eax
.no_fb:
    ret


; --- ATA PIO kernel load -------------------------------------------------------

ata_load:
    mov edi, KERNEL_PHYS
    mov ebx, KERNEL_LBA
    mov ecx, KERNEL_SECTORS
.sector:
    push ecx
    push ebx
    call ata_read_sector
    pop ebx
    pop ecx
    add edi, 512
    inc ebx
    loop .sector
    ret

; edi = destination, ebx = LBA28
ata_read_sector:
    push edi
    mov ecx, 0x200000
    mov dx, 0x1F7
.wait_bsy:
    in al, dx
    test al, 0x80
    jz .selected
    loop .wait_bsy
    jmp ata_fail
.selected:
    mov eax, ebx
    shr eax, 24
    and al, 0x0F
%if KERNEL_DRIVE_SLAVE
    or al, 0xF0                           ; kernel image on the primary slave
%else
    or al, 0xE0                           ; kernel image on the primary master
%endif
    mov dx, 0x1F6
    out dx, al
    xor eax, eax
    mov dx, 0x1F1
    out dx, al                           ; features = 0
    mov dx, 0x1F2
    mov al, 1
    out dx, al                           ; one sector
    mov eax, ebx
    mov dx, 0x1F3
    out dx, al
    mov eax, ebx
    shr eax, 8
    mov dx, 0x1F4
    out dx, al
    mov eax, ebx
    shr eax, 16
    mov dx, 0x1F5
    out dx, al
    mov dx, 0x1F7
    mov al, 0x20
    out dx, al                           ; READ SECTORS
    mov ecx, 0x200000
.wait_drq:
    in al, dx
    test al, 0x08
    jnz .ready
    test al, 0x01
    jnz ata_fail
    loop .wait_drq
    jmp ata_fail
.ready:
    mov dx, 0x1F0
    mov ecx, 256
    rep insw
    pop edi
    ret

ata_fail:
    mov esi, msg_ata_err
    call puts32
.hang:
    hlt
    jmp .hang

; --- 32-bit output helpers -----------------------------------------------------

puts32:
    push eax
    push esi
.loop:
    lodsb
    test al, al
    jz .done
    call putc32
    jmp .loop
.done:
    pop esi
    pop eax
    ret

putc32:
    push eax
    push edx
    mov ah, al
.wait:
    mov dx, 0x3FD
    in al, dx
    test al, 0x20
    jz .wait
    mov dx, 0x3F8
    mov al, ah
    out dx, al
    pop edx
    pop eax
    ret

put_dec32:
    push eax
    push ebx
    push ecx
    push edx
    mov ecx, 0
    mov ebx, 10
.loop:
    xor edx, edx
    div ebx
    push edx
    inc ecx
    test eax, eax
    jnz .loop
.emit:
    pop eax
    add al, '0'
    call putc32
    loop .emit
    pop edx
    pop ecx
    pop ebx
    pop eax
    ret

msg_ata_err: db 'E: ata read failed', 13, 10, 0
msg_mm:      db 'memmap: descriptors=', 0
msg_boot:    db 'loading kernel (ata pio) and jumping...', 13, 10, 0

BITS 64
long64:
    cld
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    cld
    mov rsp, STACK_TOP
    mov rdi, BOOTINFO_PHYS
    mov rsi, STACK_TOP
    mov rax, KERNEL_ENTRY
    jmp rax

BITS 16
