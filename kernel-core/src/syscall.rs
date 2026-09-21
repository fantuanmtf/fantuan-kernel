//! Arch-neutral syscall semantics (M9.3c). Each kernel provides the entry
//! mechanism (x86 INT 0x60, riscv ecall from U-mode) and the user-pointer
//! write bridge; the call numbers and behavior live here.
//!
//! P1 (docs/POSIX_PLAN.md) adds the POSIX-round-1 surface in append-only v2
//! numbers, routed to `vfs::posix` / `brk`. P2 adds the process/signal/VMA
//! layer (v3 numbers) routed to `process`/`signal`/`tty`. Kernels that never
//! install `user::UserMemOps` refuse the copy-dependent calls with ERR_NOSYS;
//! kernels without P2 FrameOps refuse fork/exec with ERR_NOSYS.

use fantuan_abi::{
    SYS_BRK, SYS_CHDIR, SYS_CLOCK_GETTIME, SYS_CLOSE, SYS_DUP, SYS_DUP2, SYS_ERR_INVAL,
    SYS_ERR_NOSYS, SYS_EXECVE, SYS_EXIT, SYS_FCNTL, SYS_FORK, SYS_FSTAT, SYS_GETCWD, SYS_GETDENTS,
    SYS_GETPGID, SYS_GETPGRP, SYS_GETPID, SYS_GETPPID, SYS_GET_TID, SYS_IOCTL, SYS_KILL, SYS_LSEEK,
    SYS_MKDIR, SYS_MMAP, SYS_MMAP2, SYS_MPROTECT, SYS_MUNMAP, SYS_NANOSLEEP, SYS_OK, SYS_OPEN,
    SYS_PIPE, SYS_READ, SYS_RENAME, SYS_RMDIR, SYS_SETPGID, SYS_SETSID, SYS_SIGACTION,
    SYS_SIGPROCMASK, SYS_SIGRETURN, SYS_SIGSUSPEND, SYS_SLEEP_MS, SYS_STAT, SYS_TCGETPGRP,
    SYS_TCSETPGRP, SYS_UNLINK, SYS_VERSION, SYS_WAIT4, SYS_WRITE, SYS_WRITE_FD, SYS_YIELD, Timespec,
};

use crate::process;
use crate::signal;
use crate::task;
use crate::vfs::{posix, posix_path};

pub const ABI_VERSION: u64 = 1;

/// Debug-channel write bridge (bounded copy + output; arch-specific).
pub type WriteFn = fn(u64, u64) -> u64;

/// Dispatch one call. Returns the value to place in the result register.
pub fn dispatch(write: WriteFn, n: u64, a: &[u64; 5]) -> u64 {
    match n {
        SYS_VERSION => ABI_VERSION,
        SYS_GET_TID => task::current_id(),
        SYS_EXIT => task::exit(a[0]),
        SYS_SLEEP_MS => {
            task::sleep_ms(a[0]);
            SYS_OK
        }
        SYS_WRITE => write(a[0], a[1]),
        SYS_YIELD => {
            task::schedule();
            SYS_OK
        }
        SYS_OPEN => posix::open(a[0], a[1], a[2]),
        SYS_CLOSE => posix::close(a[0]),
        SYS_READ => posix::read(a[0], a[1], a[2]),
        SYS_WRITE_FD => posix::write(a[0], a[1], a[2]),
        SYS_LSEEK => posix::lseek(a[0], a[1], a[2]),
        SYS_STAT => posix_path::stat(a[0], a[1]),
        SYS_FSTAT => posix::fstat(a[0], a[1]),
        SYS_GETDENTS => posix::getdents(a[0], a[1], a[2]),
        SYS_BRK => crate::brk::brk(a[0]),
        SYS_MMAP => posix::mmap(), // P1 stub: always ERR_NOSYS
        SYS_PIPE => posix::pipe(a[0]),
        SYS_DUP => posix::dup(a[0]),
        SYS_DUP2 => posix::dup2(a[0], a[1]),
        SYS_IOCTL => posix::ioctl(a[0], a[1], a[2]),
        SYS_CLOCK_GETTIME => posix::clock_gettime(a[0], a[1]),
        SYS_GETPID => task::current_id(),
        SYS_GETPPID => process::getppid(),
        SYS_CHDIR => posix_path::chdir(a[0]),
        SYS_GETCWD => posix_path::getcwd(a[0], a[1]),
        SYS_UNLINK => posix_path::unlink(a[0]),
        SYS_MKDIR => posix_path::mkdir(a[0], a[1]),
        SYS_RMDIR => posix_path::rmdir(a[0]),
        SYS_RENAME => posix_path::rename(a[0], a[1]),

        // P2 process layer.
        SYS_FORK => process::fork(),
        SYS_EXECVE => process::execve(a[0], a[1], a[2]),
        SYS_WAIT4 => posix::wait4(a[0], a[1], a[2], a[3]),
        SYS_KILL => process::kill(a[0] as i64, a[1] as u32),
        SYS_SIGACTION => signal::sigaction(a[0], a[1], a[2]),
        SYS_SIGPROCMASK => signal::sigprocmask(a[0], a[1], a[2]),
        SYS_SIGRETURN => signal::sigreturn(),
        SYS_SIGSUSPEND => signal::sigsuspend(a[0]),
        SYS_SETPGID => process::setpgid(a[0], a[1]),
        SYS_GETPGID => match process::getpgid(a[0]) {
            Ok(v) => v,
            Err(e) => e,
        },
        SYS_GETPGRP => process::getpgrp(),
        SYS_SETSID => process::setsid(),
        SYS_MMAP2 => process::mmap2(a[0], a[1], a[2], a[3], a[4]),
        SYS_MUNMAP => process::munmap(a[0], a[1]),
        SYS_MPROTECT => process::mprotect(a[0], a[1], a[2]),
        SYS_NANOSLEEP => nanosleep(a[0], a[1]),
        SYS_TCGETPGRP => posix::tcgetpgrp(a[0]),
        SYS_TCSETPGRP => posix::tcsetpgrp(a[0], a[1]),
        SYS_FCNTL => posix::fcntl(a[0], a[1], a[2]),
        _ => SYS_ERR_NOSYS,
    }
}

fn nanosleep(req: u64, _rem: u64) -> u64 {
    let mut t = Timespec::default();
    let bytes = unsafe {
        core::slice::from_raw_parts_mut(&mut t as *mut Timespec as *mut u8, core::mem::size_of::<Timespec>())
    };
    if crate::user::copy_in(bytes, req).is_none() {
        return fantuan_abi::SYS_ERR_FAULT;
    }
    if t.tv_sec < 0 || t.tv_nsec < 0 || t.tv_nsec >= 1_000_000_000 {
        return SYS_ERR_INVAL;
    }
    let ms = (t.tv_sec as u64) * 1000 + (t.tv_nsec as u64) / 1_000_000;
    if ms > 0 {
        task::sleep_ms(ms);
    }
    SYS_OK
}
