//! fantuan-user — the first userland program (M4).
//!
//! Runs in ring 3; the only way to reach the kernel is the syscall ABI
//! (INT 0x60, numbers in fantuan-abi).

#![no_std]
#![no_main]

#[cfg(target_arch = "x86_64")]
mod arch {
    use core::arch::global_asm;

    global_asm!(
        ".global _start",
        "_start:",
        "    call    user_main",
        "    ud2",
        // Syscall trampoline: reserve scratch below rsp, repack SysV args into
        // rax = number, rdi..r8 = args (same shape as the kernel-side one).
        ".global syscall_trampoline",
        "syscall_trampoline:",
        "    sub     rsp, 128",
        "    mov     rax, rdi",
        "    mov     rdi, rsi",
        "    mov     rsi, rdx",
        "    mov     rdx, rcx",
        "    mov     rcx, r8",
        "    mov     r8, r9",
        "    int     0x60",
        "    add     rsp, 128",
        "    ret",
    );

    extern "C" {
        fn syscall_trampoline(n: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64;
    }

    pub fn syscall(n: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64 {
        unsafe { syscall_trampoline(n, a1, a2, a3, a4, a5) }
    }
}

#[cfg(target_arch = "riscv64")]
mod arch {
    use core::arch::{asm, global_asm};

    global_asm!(
        ".global _start",
        "_start:",
        "    call    user_main",
        "    unimp",
    );

    /// ecall: a7 = number, a0..a4 = args, result in a0 (M9.3c).
    pub fn syscall(n: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> u64 {
        let ret: u64;
        unsafe {
            asm!(
                "ecall",
                in("a7") n,
                in("a0") a1,
                in("a1") a2,
                in("a2") a3,
                in("a3") a4,
                in("a4") a5,
                lateout("a0") ret,
                options(nostack),
            );
        }
        ret
    }
}

use arch::syscall;

fn push_str(buf: &mut [u8], mut off: usize, s: &str) -> usize {
    for &b in s.as_bytes() {
        if off < buf.len() {
            buf[off] = b;
            off += 1;
        }
    }
    off
}

fn push_u64(buf: &mut [u8], mut off: usize, mut v: u64) -> usize {
    if v == 0 {
        buf[off] = b'0';
        return off + 1;
    }
    let mut tmp = [0u8; 20];
    let mut n = 0;
    while v > 0 {
        tmp[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
    }
    while n > 0 {
        n -= 1;
        buf[off] = tmp[n];
        off += 1;
    }
    off
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // No formatting machinery in userland: just report and die.
    let msg = b"userland: panic\n";
    syscall(fantuan_abi::SYS_WRITE, msg.as_ptr() as u64, msg.len() as u64, 0, 0, 0);
    syscall(fantuan_abi::SYS_EXIT, 1, 0, 0, 0, 0);
    loop {
        core::hint::spin_loop();
    }
}

#[no_mangle]
pub extern "C" fn user_main() -> ! {
    let tid = syscall(fantuan_abi::SYS_GET_TID, 0, 0, 0, 0, 0);
    let mut n = 0u64;
    while n < 3 {
        let mut buf = [0u8; 96];
        let mut off = 0;
        off = push_str(&mut buf, off, "userland: hello from tid ");
        off = push_u64(&mut buf, off, tid);
        off = push_str(&mut buf, off, " (loop ");
        off = push_u64(&mut buf, off, n);
        off = push_str(&mut buf, off, ")\n");
        syscall(fantuan_abi::SYS_WRITE, buf.as_ptr() as u64, off as u64, 0, 0, 0);
        n += 1;
        syscall(fantuan_abi::SYS_SLEEP_MS, 500, 0, 0, 0, 0);
    }
    let mut buf = [0u8; 64];
    let mut off = 0;
    off = push_str(&mut buf, off, "userland: tid ");
    off = push_u64(&mut buf, off, tid);
    off = push_str(&mut buf, off, " exiting\n");
    syscall(fantuan_abi::SYS_WRITE, buf.as_ptr() as u64, off as u64, 0, 0, 0);
    syscall(fantuan_abi::SYS_EXIT, 0, 0, 0, 0, 0);
    loop {
        core::hint::spin_loop();
    }
}
