/* libc-fantuan — sys/ioctl.h (P1): TCGETS/TCSETS/TIOCGWINSZ on /dev/console. */
#ifndef _SYS_IOCTL_H
#define _SYS_IOCTL_H

#include <sys/types.h>
#include <termios.h>
#include <fantuan/abi.h>

#define TCGETS FANTUAN_IOCTL_TCGETS
#define TCSETS FANTUAN_IOCTL_TCSETS
#define TIOCGWINSZ FANTUAN_IOCTL_TIOCGWINSZ
#define FIONREAD 0x541B
#define TIOCNOTTY 0x5422

struct winsize {
    unsigned short ws_row;
    unsigned short ws_col;
    unsigned short ws_xpixel;
    unsigned short ws_ypixel;
};

int ioctl(int fd, unsigned long request, ...);

#endif /* _SYS_IOCTL_H */
