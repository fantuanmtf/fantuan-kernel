/* libc-fantuan — internal native-ABI definitions (P1).
 *
 * Mirrors abi/src/lib.rs and abi/src/posix.rs. Syscalls return a small
 * non-negative value on success or a negative Fantuan errno (see errno.h;
 * the numbering is Fantuan-native, not Linux's). This header is internal:
 * public headers include it, programs should not need it directly. */
#ifndef _FANTUAN_ABI_H
#define _FANTUAN_ABI_H

#define FANTUAN_SYS_VERSION 0
#define FANTUAN_SYS_EXIT 1
#define FANTUAN_SYS_SLEEP_MS 2
#define FANTUAN_SYS_WRITE 3 /* v1 debug channel: (buf, len) */
#define FANTUAN_SYS_GET_TID 4
#define FANTUAN_SYS_YIELD 5

#define FANTUAN_SYS_OPEN 6
#define FANTUAN_SYS_CLOSE 7
#define FANTUAN_SYS_READ 8
#define FANTUAN_SYS_WRITE_FD 9
#define FANTUAN_SYS_LSEEK 10
#define FANTUAN_SYS_STAT 11
#define FANTUAN_SYS_FSTAT 12
#define FANTUAN_SYS_GETDENTS 13
#define FANTUAN_SYS_BRK 14
#define FANTUAN_SYS_MMAP 15
#define FANTUAN_SYS_PIPE 16
#define FANTUAN_SYS_DUP 17
#define FANTUAN_SYS_DUP2 18
#define FANTUAN_SYS_IOCTL 19
#define FANTUAN_SYS_CLOCK_GETTIME 20
#define FANTUAN_SYS_GETPID 21
#define FANTUAN_SYS_GETPPID 22
#define FANTUAN_SYS_CHDIR 23
#define FANTUAN_SYS_GETCWD 24
#define FANTUAN_SYS_UNLINK 25
#define FANTUAN_SYS_MKDIR 26
#define FANTUAN_SYS_RMDIR 27
#define FANTUAN_SYS_RENAME 28

/* P2 (POSIX round 2) syscalls: process/signal/VMA surface. */
#define FANTUAN_SYS_FORK 29
#define FANTUAN_SYS_EXECVE 30
#define FANTUAN_SYS_WAIT4 31
#define FANTUAN_SYS_KILL 32
#define FANTUAN_SYS_SIGACTION 33
#define FANTUAN_SYS_SIGPROCMASK 34
#define FANTUAN_SYS_SIGRETURN 35
#define FANTUAN_SYS_SETPGID 36
#define FANTUAN_SYS_GETPGID 37
#define FANTUAN_SYS_GETPGRP 38
#define FANTUAN_SYS_SETSID 39
#define FANTUAN_SYS_MMAP2 40
#define FANTUAN_SYS_MUNMAP 41
#define FANTUAN_SYS_MPROTECT 42
#define FANTUAN_SYS_NANOSLEEP 43
#define FANTUAN_SYS_TCGETPGRP 44
#define FANTUAN_SYS_TCSETPGRP 45
#define FANTUAN_SYS_SIGSUSPEND 46
#define FANTUAN_SYS_FCNTL 47

/* ioctl requests implemented on /dev/console (Linux-compatible numbers). */
#define FANTUAN_IOCTL_TCGETS 0x5401
#define FANTUAN_IOCTL_TCSETS 0x5402
#define FANTUAN_IOCTL_TIOCGPGRP 0x540F
#define FANTUAN_IOCTL_TIOCSPGRP 0x5410
#define FANTUAN_IOCTL_TIOCGWINSZ 0x5413

/* P1 layouts: field order/widths must match abi/src/posix.rs exactly. */
struct fantuan_stat {
    unsigned long st_dev;
    unsigned long st_ino;
    unsigned long st_size;
    unsigned long st_blocks;
    unsigned long st_blksize;
    unsigned int st_mode;
    unsigned int st_nlink;
    unsigned int st_uid;
    unsigned int st_gid;
    unsigned long st_atime_ns;
    unsigned long st_mtime_ns;
    unsigned long st_ctime_ns;
};

struct fantuan_timespec {
    long tv_sec;
    long tv_nsec;
};

struct fantuan_dirent {
    unsigned long d_ino;
    unsigned int d_type;
    unsigned int d_reclen;
    char d_name[56];
};

struct fantuan_termios {
    unsigned int c_iflag;
    unsigned int c_oflag;
    unsigned int c_cflag;
    unsigned int c_lflag;
    unsigned char c_cc[32];
};

struct fantuan_winsize {
    unsigned short ws_row;
    unsigned short ws_col;
    unsigned short ws_xpixel;
    unsigned short ws_ypixel;
};

/* P2 sigaction payload (matches abi/src/posix.rs `SigAction`). */
struct fantuan_sigaction {
    unsigned long sa_handler;
    unsigned long sa_mask;
    int sa_flags;
    int sa_pad;
    unsigned long sa_restorer;
};

/* Raw call: n and up to five arguments, returns the raw result. */
long __fantuan_syscall6(long n, long a1, long a2, long a3, long a4, long a5);

#endif /* _FANTUAN_ABI_H */
