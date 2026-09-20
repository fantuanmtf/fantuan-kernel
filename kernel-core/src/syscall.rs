//! Arch-neutral syscall semantics (M9.3c). Each kernel provides the entry
//! mechanism (x86 int 0x60, riscv ecall from U-mode) and the user-pointer
//! write bridge; the call numbers and behavior live here.
//!
//! P1 (docs/POSIX_PLAN.md) adds the POSIX-round-1 surface in append-only
//! v2 numbers, routed to `vfs::posix` / `brk`. Kernels that never install
//! `user::UserMemOps` refuse the copy-dependent calls with ERR_NOSYS.

use fantuan_abi::{
    SYS_BRK, SYS_CHDIR, SYS_CLOCK_GETTIME, SYS_CLOSE, SYS_DUP, SYS_DUP2, SYS_ERR_NOSYS, SYS_EXIT,
    SYS_FSTAT, SYS_GETCWD, SYS_GETDENTS, SYS_GETPID, SYS_GETPPID, SYS_GET_TID, SYS_IOCTL, SYS_LSEEK,
    SYS_MKDIR, SYS_MMAP, SYS_OK, SYS_OPEN, SYS_PIPE, SYS_READ, SYS_RENAME, SYS_RMDIR, SYS_SLEEP_MS,
    SYS_STAT, SYS_UNLINK, SYS_VERSION, SYS_WRITE, SYS_WRITE_FD, SYS_YIELD,
};

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
        SYS_MMAP => posix::mmap(),
        SYS_PIPE => posix::pipe(a[0]),
        SYS_DUP => posix::dup(a[0]),
        SYS_DUP2 => posix::dup2(a[0], a[1]),
        SYS_IOCTL => posix::ioctl(a[0], a[1], a[2]),
        SYS_CLOCK_GETTIME => posix::clock_gettime(a[0], a[1]),
        SYS_GETPID => task::current_id(),
        SYS_GETPPID => 0, // no fork yet (P2)
        SYS_CHDIR => posix_path::chdir(a[0]),
        SYS_GETCWD => posix_path::getcwd(a[0], a[1]),
        SYS_UNLINK => posix_path::unlink(a[0]),
        SYS_MKDIR => posix_path::mkdir(a[0], a[1]),
        SYS_RMDIR => posix_path::rmdir(a[0]),
        SYS_RENAME => posix_path::rename(a[0], a[1]),
        _ => SYS_ERR_NOSYS,
    }
}
