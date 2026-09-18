/* fantuan-kernel — syscall.S (M3)
 * Syscall trampoline (DESIGN.md §4.6): fn(n, a1..a5) -> u64.
 * Reserves scratch below rsp so the INT 0x60 frame cannot touch the caller's
 * red zone, then repacks the SysV args into the syscall register ABI:
 * rax = number, rdi..r8 = args. Returns rax.
 * Intel syntax; compiled by cc via kernel/build.rs. */

    .intel_syntax noprefix

    .global syscall_trampoline
syscall_trampoline:
    sub     rsp, 128
    mov     rax, rdi        /* number */
    mov     rdi, rsi
    mov     rsi, rdx
    mov     rdx, rcx
    mov     rcx, r8
    mov     r8, r9
    int     0x60
    add     rsp, 128
    ret
