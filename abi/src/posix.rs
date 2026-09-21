//! POSIX-round-1 payload structs shared by the kernel and libc-fantuan.
//!
//! These are Fantuan-native layouts, NOT Linux's. libc-fantuan's headers
//! (`sys/stat.h`, `time.h`, `dirent.h`, `termios.h`) mirror the field order
//! and widths here exactly; the P1 sizes are asserted in `libc-fantuan`'s
//! build and by the kernel tests. Times are nanoseconds since boot (there is
//! no RTC yet).

/// P1 stat result, 8-byte aligned (80 bytes on both sides).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_size: u64,
    pub st_blocks: u64,
    pub st_blksize: u64,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_atime_ns: u64,
    pub st_mtime_ns: u64,
    pub st_ctime_ns: u64,
}

/// P1 timespec (16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Timespec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

/// P1 dirent: one fixed 72-byte record per getdents call, 0 at end of
/// directory.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Dirent {
    pub d_ino: u64,
    pub d_type: u32,
    pub d_reclen: u32,
    pub d_name: [u8; 56],
}

/// P1 termios: the serial console reports ICANON|ECHO|ISIG; writes are
/// accepted and ignored (no line discipline yet).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Termios {
    pub c_iflag: u32,
    pub c_oflag: u32,
    pub c_cflag: u32,
    pub c_lflag: u32,
    pub c_cc: [u8; 32],
}

/// P1 winsize for TIOCGWINSZ (80x25 reported).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Winsize {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

/// P2 sigaction payload. Field order/widths must match libc-fantuan's
/// signal.h `struct sigaction` exactly (32 bytes on x86_64). `sa_restorer`
/// is filled in by libc's sigaction wrapper with the trampoline that issues
/// SYS_SIGRETURN; the kernel pushes it as the handler's return address.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SigAction {
    pub sa_handler: u64,
    pub sa_mask: u64,
    pub sa_flags: i32,
    pub _pad: i32,
    pub sa_restorer: u64,
}

impl Default for SigAction {
    fn default() -> Self {
        SigAction { sa_handler: 0, sa_mask: 0, sa_flags: 0, _pad: 0, sa_restorer: 0 }
    }
}

/// P2 signal frame pushed on the user stack before a handler runs. Exactly
/// the GP-register order of the x86_64 InterruptFrame (minus vector/error)
/// followed by the interrupted RIP/RFLAGS/RSP; SYS_SIGRETURN reads it back
/// from the user stack. The kernel writes it with copy_out.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SignalFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub rip: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub signum: u64,
    pub mask: u64,
}
