/* libc-fantuan — termios.h (P1). Layout matches abi/src/posix.rs; the serial
 * console reports ICANON|ECHO|ISIG and accepts writes (no line discipline). */
#ifndef _TERMIOS_H
#define _TERMIOS_H

#include <sys/types.h>

typedef unsigned char cc_t;
typedef unsigned int speed_t;
typedef unsigned int tcflag_t;

#define NCCS 32

struct termios {
    tcflag_t c_iflag;
    tcflag_t c_oflag;
    tcflag_t c_cflag;
    tcflag_t c_lflag;
    cc_t c_cc[NCCS];
};

/* iflag */
#define IGNBRK 0000001
#define BRKINT 0000002
#define IGNPAR 0000004
#define PARMRK 0000010
#define INPCK 0000020
#define ISTRIP 0000040
#define INLCR 0000100
#define IGNCR 0000200
#define ICRNL 0000400
#define IXON 0002000
/* oflag */
#define OPOST 0000001
#define ONLCR 0000004
/* cflag */
#define CSIZE 0000060
#define CS5 0000000
#define CS6 0000020
#define CS7 0000040
#define CS8 0000060
#define CREAD 0000200
#define HUPCL 0002000
#define CLOCAL 0004000
/* lflag */
#define ISIG 0000001
#define ICANON 0000002
#define ECHO 0000010
#define ECHOE 0000020
#define ECHOK 0000040
#define ECHONL 0000100
#define NOFLSH 0000200
#define IEXTEN 0100000
/* c_cc indices */
#define VEOF 4
#define VEOL 11
#define VERASE 2
#define VINTR 0
#define VKILL 3
#define VMIN 6
#define VQUIT 1
#define VTIME 5
#define VSTART 8
#define VSTOP 7
#define VSUSP 10

#define TCSANOW 0
#define TCSADRAIN 1
#define TCSAFLUSH 2

speed_t cfgetispeed(const struct termios *t);
speed_t cfgetospeed(const struct termios *t);
int cfsetispeed(struct termios *t, speed_t speed);
int cfsetospeed(struct termios *t, speed_t speed);
int tcgetattr(int fd, struct termios *t);
int tcsetattr(int fd, int optional_actions, const struct termios *t);
int tcflush(int fd, int queue_selector);
int tcdrain(int fd);
pid_t tcgetpgrp(int fd);
int tcsetpgrp(int fd, pid_t pgrp);

#endif /* _TERMIOS_H */
