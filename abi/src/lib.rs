//! fantuan-abi — the versioned ABI between the bootloader and the kernel.
//!
//! See docs/DESIGN.md §4.1. Append-only: fields may only ever be added, never
//! removed or reordered. Any change here breaks the boot chain on both sides —
//! this crate exists so the struct is defined in exactly one place.

#![no_std]

/// POSIX-round-1 syscall payload structs (P1). The boot ABI and the syscall
/// numbers stay in this file; the C-visible layouts live in `posix` so both
/// sides (kernel and libc headers) have one documented source.
pub mod posix;
pub use posix::*;

pub const BOOT_MAGIC: u32 = 0x4654_4E46; // "FTNF"
/// v2 (M9.2): append-only arch/hartid/dtb fields for the RISC-V boot path.
pub const BOOT_VERSION: u32 = 2;

/// Physical-to-virtual mapping convention (DESIGN.md §4.5): every physical
/// address p is reachable at PHYS_OFFSET + p. Shared by the bootloader (which
/// builds the initial page tables) and the kernel (which is linked there).
#[cfg(target_arch = "x86_64")]
pub const PHYS_OFFSET: u64 = 0xFFFF_8000_0000_0000; // -2 GiB, Linux-style
/// Sv39's canonical high half (bits 63:39 sign-extend bit 38).
#[cfg(target_arch = "riscv64")]
pub const PHYS_OFFSET: u64 = 0xFFFF_FFC0_0000_0000;
/// arm64 linear map (M11 R9a): the top 16 bits select TTBR1 with T1SZ=16
/// (48-bit VA); the kernel's identity map stays in TTBR0.
#[cfg(target_arch = "aarch64")]
pub const PHYS_OFFSET: u64 = 0xFFFF_0000_0000_0000;
/// i686 (M10): 3G/1G split; the direct map covers the first 1 GiB of RAM
/// (see docs/M10_BOOT_32BIT.md 7.5).
#[cfg(target_arch = "x86")]
pub const PHYS_OFFSET: u64 = 0xC000_0000;

/// Pages of initial kernel stack allocated by the bootloader.
pub const BOOT_STACK_PAGES: u64 = 16;

// --- Syscall ABI v1 (shared by kernel and userland) ------------------------

pub const SYS_VERSION: u64 = 0;
pub const SYS_EXIT: u64 = 1;
pub const SYS_SLEEP_MS: u64 = 2;
pub const SYS_WRITE: u64 = 3; // (buf, len): kernel debug channel (serial)
pub const SYS_GET_TID: u64 = 4;
pub const SYS_YIELD: u64 = 5;

pub const SYS_OK: u64 = 0;
pub const SYS_ERR_NOSYS: u64 = u64::MAX; // -1
pub const SYS_ERR_INVAL: u64 = u64::MAX - 1; // -2

// --- Syscall additions v2: POSIX round 1 (P1, append-only) -----------------
//
// SYS_WRITE (3) keeps its v1 debug-channel (buf, len) semantics forever;
// fd-based writes use SYS_WRITE_FD (9). Numbers are append-only, never
// renumbered. `SYS_VERSION` still reports ABI_VERSION = 1: the new calls are
// additive, so v1 consumers keep working; a consumer that wants the file
// surface just calls the new numbers. See docs/POSIX_PLAN.md.
//
// Errors are negative Fantuan-native codes in the u64 two's-complement
// space, NOT Linux's; libc-fantuan's errno.h mirrors this list. P1 has no
// security model: EACCES/EPERM are placeholders, everything runs as root.

pub const SYS_OPEN: u64 = 6; // (path, flags, mode) -> fd
pub const SYS_CLOSE: u64 = 7; // (fd) -> 0
pub const SYS_READ: u64 = 8; // (fd, buf, len) -> bytes
pub const SYS_WRITE_FD: u64 = 9; // (fd, buf, len) -> bytes
pub const SYS_LSEEK: u64 = 10; // (fd, off, whence) -> offset
pub const SYS_STAT: u64 = 11; // (path, statbuf) -> 0
pub const SYS_FSTAT: u64 = 12; // (fd, statbuf) -> 0
pub const SYS_GETDENTS: u64 = 13; // (fd, direntbuf) -> 1|0 (Fantuan dirent)
pub const SYS_BRK: u64 = 14; // (addr|0) -> current break
pub const SYS_MMAP: u64 = 15; // P1 stub: returns SYS_ERR_NOSYS
pub const SYS_PIPE: u64 = 16; // (int[2]) -> 0
pub const SYS_DUP: u64 = 17; // (fd) -> new fd
pub const SYS_DUP2: u64 = 18; // (old, new) -> new
pub const SYS_IOCTL: u64 = 19; // (fd, req, arg) -> 0
pub const SYS_CLOCK_GETTIME: u64 = 20; // (clockid, timespec*) -> 0
pub const SYS_GETPID: u64 = 21;
pub const SYS_GETPPID: u64 = 22; // 0 until fork lands (P2)
pub const SYS_CHDIR: u64 = 23; // (path) -> 0
pub const SYS_GETCWD: u64 = 24; // (buf, len) -> length
pub const SYS_UNLINK: u64 = 25; // (path) -> 0
pub const SYS_MKDIR: u64 = 26; // (path, mode) -> 0
pub const SYS_RMDIR: u64 = 27; // (path) -> 0
pub const SYS_RENAME: u64 = 28; // (old, new) -> 0

/// First P1 syscall number; dispatch routes n >= this to the POSIX layer.
pub const SYS_V2_FIRST: u64 = SYS_OPEN;

/// Highest P1 call (stubs included). Append new calls after it.
pub const SYS_V2_LAST: u64 = SYS_RENAME;

// --- Syscall additions v3: POSIX round 2 (P2, append-only) -----------------
//
// P2 adds the process/signal/VMA surface (docs/POSIX_PLAN.md). P1 numbers are
// untouched; `SYS_MMAP` (15) stays the ENOSYS stub forever and the real call
// is `SYS_MMAP2` (40) with the full argument set. Frame-dependent calls
// (fork/execve/sigreturn) are handled by the arch that installs process
// FrameOps; kernels without it report ERR_NOSYS.

pub const SYS_FORK: u64 = 29; // () -> child pid (0 in the child)
pub const SYS_EXECVE: u64 = 30; // (path, argv, envp) -> only returns on error
pub const SYS_WAIT4: u64 = 31; // (pid, status, options, rusage) -> pid
pub const SYS_KILL: u64 = 32; // (pid, sig) -> 0
pub const SYS_SIGACTION: u64 = 33; // (signum, act, oldact) -> 0
pub const SYS_SIGPROCMASK: u64 = 34; // (how, set, oldset) -> 0
pub const SYS_SIGRETURN: u64 = 35; // () -> restores the interrupted context
pub const SYS_SETPGID: u64 = 36; // (pid, pgid) -> 0
pub const SYS_GETPGID: u64 = 37; // (pid) -> pgid
pub const SYS_GETPGRP: u64 = 38; // () -> pgid
pub const SYS_SETSID: u64 = 39; // () -> sid
pub const SYS_MMAP2: u64 = 40; // (addr, len, prot, flags, fd) -> addr
pub const SYS_MUNMAP: u64 = 41; // (addr, len) -> 0
pub const SYS_MPROTECT: u64 = 42; // (addr, len, prot) -> 0
pub const SYS_NANOSLEEP: u64 = 43; // (req, rem) -> 0
pub const SYS_TCGETPGRP: u64 = 44; // (fd) -> pgrp
pub const SYS_TCSETPGRP: u64 = 45; // (fd, pgrp) -> 0
pub const SYS_SIGSUSPEND: u64 = 46; // (mask) -> 0 after a signal arrives
pub const SYS_FCNTL: u64 = 47; // (fd, cmd, arg) -> varies

/// First P2 syscall number; the kernel-core router checks n >= this.
pub const SYS_P2_FIRST: u64 = SYS_FORK;

/// Highest P2 call (append new calls after it).
pub const SYS_P2_LAST: u64 = SYS_FCNTL;

// --- P2 process/signal/VMA constants ---------------------------------------

/// Signal numbers (mirror libc-fantuan/signal.h and POSIX).
pub const SIGHUP: u32 = 1;
pub const SIGINT: u32 = 2;
pub const SIGQUIT: u32 = 3;
pub const SIGILL: u32 = 4;
pub const SIGTRAP: u32 = 5;
pub const SIGABRT: u32 = 6;
pub const SIGBUS: u32 = 7;
pub const SIGFPE: u32 = 8;
pub const SIGKILL: u32 = 9;
pub const SIGUSR1: u32 = 10;
pub const SIGSEGV: u32 = 11;
pub const SIGUSR2: u32 = 12;
pub const SIGPIPE: u32 = 13;
pub const SIGALRM: u32 = 14;
pub const SIGTERM: u32 = 15;
pub const SIGCHLD: u32 = 17;
pub const SIGCONT: u32 = 18;
pub const SIGSTOP: u32 = 19;
pub const SIGTSTP: u32 = 20;
pub const SIGTTIN: u32 = 21;
pub const SIGTTOU: u32 = 22;
pub const SIGWINCH: u32 = 28;
pub const NSIG: u32 = 32;

/// Handler sentinels (the canonical SIG_DFL/SIG_IGN values).
pub const SIG_DFL: u64 = 0;
pub const SIG_IGN: u64 = 1;

/// sigprocmask how values (POSIX).
pub const SIG_BLOCK: u64 = 0;
pub const SIG_UNBLOCK: u64 = 1;
pub const SIG_SETMASK: u64 = 2;

/// mmap protections and flags (POSIX/Linux-compatible numbers).
pub const PROT_NONE: u64 = 0;
pub const PROT_READ: u64 = 1;
pub const PROT_WRITE: u64 = 2;
pub const PROT_EXEC: u64 = 4;
pub const MAP_SHARED: u64 = 0x01;
pub const MAP_PRIVATE: u64 = 0x02;
pub const MAP_FIXED: u64 = 0x10;
pub const MAP_ANONYMOUS: u64 = 0x20;

/// wait4 options (mirror libc-fantuan/sys/wait.h).
pub const WNOHANG: u64 = 1;
pub const WUNTRACED: u64 = 2;

/// Base of the P2 mmap region; brk grows from USER_HEAP_BASE upward and the
/// loader's ET_EXEC images stay well below both.
pub const USER_MMAP_BASE: u64 = 0x1000_0000;

/// Per-process VMA slots (static table, no kernel heap).
pub const MAX_VMAS: usize = 8;

pub const SYS_ERR_NOENT: u64 = u64::MAX - 2; // -3
pub const SYS_ERR_BADF: u64 = u64::MAX - 3; // -4
pub const SYS_ERR_IO: u64 = u64::MAX - 4; // -5
pub const SYS_ERR_NOMEM: u64 = u64::MAX - 5; // -6
pub const SYS_ERR_ACCES: u64 = u64::MAX - 6; // -7
pub const SYS_ERR_EXIST: u64 = u64::MAX - 7; // -8
pub const SYS_ERR_NOTDIR: u64 = u64::MAX - 8; // -9
pub const SYS_ERR_ISDIR: u64 = u64::MAX - 9; // -10
pub const SYS_ERR_NOTEMPTY: u64 = u64::MAX - 10; // -11
pub const SYS_ERR_RANGE: u64 = u64::MAX - 11; // -12
pub const SYS_ERR_SPIPE: u64 = u64::MAX - 12; // -13
pub const SYS_ERR_NOTTY: u64 = u64::MAX - 13; // -14
pub const SYS_ERR_MFILE: u64 = u64::MAX - 14; // -15
pub const SYS_ERR_FAULT: u64 = u64::MAX - 15; // -16
pub const SYS_ERR_AGAIN: u64 = u64::MAX - 16; // -17
pub const SYS_ERR_PIPE: u64 = u64::MAX - 17; // -18
pub const SYS_ERR_ROFS: u64 = u64::MAX - 18; // -19
pub const SYS_ERR_NODEV: u64 = u64::MAX - 19; // -20
pub const SYS_ERR_NAMETOOLONG: u64 = u64::MAX - 20; // -21
pub const SYS_ERR_LOOP: u64 = u64::MAX - 21; // -22
pub const SYS_ERR_FBIG: u64 = u64::MAX - 22; // -23
pub const SYS_ERR_NOSPC: u64 = u64::MAX - 23; // -24
pub const SYS_ERR_INTR: u64 = u64::MAX - 24; // -25
pub const SYS_ERR_CHILD: u64 = u64::MAX - 25; // -26
pub const SYS_ERR_PERM: u64 = u64::MAX - 26; // -27
pub const SYS_ERR_SRCH: u64 = u64::MAX - 27; // -28
pub const SYS_ERR_NOEXEC: u64 = u64::MAX - 28; // -29 (P2: exec format)

/// User heap base for P1 (the ELF images link at 0x400000 and are tiny).
/// The ELF loader's segment end is not consulted yet; P2 derives it.
pub const USER_HEAP_BASE: u64 = 0x50_0000;

/// Open flags (mirrored by libc-fantuan's fcntl.h).
pub const O_RDONLY: u64 = 0;
pub const O_WRONLY: u64 = 1;
pub const O_RDWR: u64 = 2;
pub const O_CREAT: u64 = 0x40;
pub const O_EXCL: u64 = 0x80;
pub const O_TRUNC: u64 = 0x200;
pub const O_APPEND: u64 = 0x400;
pub const O_NONBLOCK: u64 = 0x800;
pub const O_CLOEXEC: u64 = 0x80000;

/// fcntl commands and the close-on-exec flag (mirror libc-fantuan/fcntl.h).
pub const F_DUPFD: u64 = 0;
pub const F_GETFD: u64 = 1;
pub const F_SETFD: u64 = 2;
pub const F_GETFL: u64 = 3;
pub const F_SETFL: u64 = 4;
pub const FD_CLOEXEC: u64 = 1;

/// mode bits (mirrored by sys/stat.h; POSIX values).
pub const S_IFMT: u32 = 0xF000;
pub const S_IFDIR: u32 = 0x4000;
pub const S_IFCHR: u32 = 0x2000;
pub const S_IFREG: u32 = 0x8000;
pub const S_IFIFO: u32 = 0x1000;

/// dirent d_type values (POSIX).
pub const DT_UNKNOWN: u32 = 0;
pub const DT_FIFO: u32 = 1;
pub const DT_CHR: u32 = 2;
pub const DT_DIR: u32 = 4;
pub const DT_REG: u32 = 8;

/// ioctl requests (Linux-compatible numbers; libc-fantuan's termios.h and
/// sys/ioctl.h mirror them). P1 implements them on /dev/console only.
pub const IOCTL_TCGETS: u64 = 0x5401;
pub const IOCTL_TCSETS: u64 = 0x5402;
pub const IOCTL_TIOCGPGRP: u64 = 0x540F;
pub const IOCTL_TIOCSPGRP: u64 = 0x5410;
pub const IOCTL_TIOCGWINSZ: u64 = 0x5413;

// P1 payload structs (Stat/Timespec/Dirent/Termios/Winsize) live in the
// `posix` module and are re-exported above, so the kernel side and the C
// headers document one layout.

// --- User-mode layout (M4) -------------------------------------------------

/// User data segment selector (GDT index 5, RPL 3).
pub const USER_DS_SEL: u16 = 0x2B;
/// User code segment selector (GDT index 6, RPL 3).
pub const USER_CS_SEL: u16 = 0x33;
/// User stack top (4 frames mapped at this base).
pub const USER_STACK_TOP: u64 = 0x300000 + 4 * 4096;

/// EFI memory type for free, usable RAM.
pub const MEMORY_TYPE_CONVENTIONAL: u32 = 7;

/// Must match the EFI_MEMORY_DESCRIPTOR layout the firmware returns. The
/// explicit pad keeps the offsets identical on 32-bit targets, where a u64
/// field would otherwise align to 4 bytes (M10 i686).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemoryDescriptor {
    pub type_: u32,
    pub _pad0: u32,
    pub physical_start: u64,
    pub virtual_start: u64,
    pub number_of_pages: u64,
    pub attribute: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MemMap {
    pub ptr: *const MemoryDescriptor,
    pub count: usize,
    pub desc_size: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FrameBuffer {
    pub base: u64,
    pub size: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32, // pixels per scanline
    pub format: u32, // 0=RGB8 1=BGR8 2=bitmask
}

#[cfg(target_pointer_width = "64")]
#[repr(C)]
pub struct BootInfo {
    pub magic: u32,
    pub version: u32,
    pub memmap: MemMap,
    pub fb: FrameBuffer,
    pub rsdp: u64,
    pub kernel_base: u64,
    pub stack_top: u64,
    pub caps: u64,
    // M2 additions (append-only; older fields never move):
    /// Physical address of the bootloader-built initial PML4.
    pub boot_pml4: u64,
    /// Number of 4K pages the bootloader's page tables occupy (the frame
    /// allocator must keep them used until the kernel switches away).
    pub boot_tables_pages: u64,
    // M7.5 addition:
    /// Physical address of the UEFI Runtime Services table. Runtime services
    /// survive ExitBootServices; the kernel calls them with physical
    /// addresses (no SetVirtualAddressMap; the 4 GiB identity map covers it).
    pub runtime_services: u64,
    // M5.5 addition:
    /// Physical address of the SMBIOS entry point structure found in the UEFI
    /// configuration table (SMBIOS3 preferred), or 0 when the firmware
    /// publishes none. The kernel parses the structure table from here.
    pub smbios_table: u64,
    // M9.2 additions (append-only):
    /// Boot path discriminator: 1 = x86_64 UEFI, 2 = riscv64 OpenSBI,
    /// 3 = BIOS (x86_64 or i686), 4 = aarch64 direct FDT (QEMU virt).
    pub arch: u32,
    /// RISC-V: hart id from the OpenSBI handoff (0 on x86_64/aarch64).
    pub hartid: u64,
    /// RISC-V: physical DTB address from the OpenSBI handoff (0 on x86_64);
    /// aarch64: the DTB address QEMU passed in x0.
    pub dtb: u64,
}

/// 32-bit BootInfo (i686 BIOS): the same field order with pointer-width
/// `usize` fields and 4-byte u64 alignment. Stage2 writes these offsets for
/// the i686 build (docs/M10_BOOT_32BIT.md 7.6).
#[cfg(target_pointer_width = "32")]
#[repr(C)]
pub struct BootInfo {
    pub magic: u32,
    pub version: u32,
    pub memmap: MemMap,
    pub fb: FrameBuffer,
    pub rsdp: u64,
    pub kernel_base: u64,
    pub stack_top: u64,
    pub caps: u64,
    pub boot_pml4: u64,
    pub boot_tables_pages: u64,
    pub runtime_services: u64,
    pub smbios_table: u64,
    pub arch: u32,
    pub hartid: u64,
    pub dtb: u64,
    // M10-5 additions (append-only, 32-bit BIOS BootInfo only):
    /// Physical address of the VBE linear framebuffer, or 0 when VBE is
    /// unavailable (the kernel falls back to the serial console).
    pub fb_phys: u64,
    /// Framebuffer bytes per scanline.
    pub fb_pitch: u32,
    /// Framebuffer width in pixels.
    pub fb_width: u16,
    /// Framebuffer height in scanlines.
    pub fb_height: u16,
    /// Framebuffer bits per pixel (16/24/32 are console-supported).
    pub fb_bpp: u8,
    pub _pad_fb: [u8; 3],
    /// Physical address of the BIOS-copied 8x16 font, or 0 when VBE failed.
    pub fb_font_phys: u32,
}

#[cfg(target_pointer_width = "32")]
const _: () = assert!(core::mem::size_of::<BootInfo>() == 160);
