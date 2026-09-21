/* libc-fantuan — termios (P1): the serial console reports defaults and
 * accepts sets (no line discipline). Process-group calls arrive in P2. */
#include <errno.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>
#include <fantuan/abi.h>

long __fantuan_raw(long n, long a1, long a2, long a3, long a4, long a5);

speed_t cfgetispeed(const struct termios *t)
{
    (void)t;
    return 0;
}

speed_t cfgetospeed(const struct termios *t)
{
    (void)t;
    return 0;
}

int cfsetispeed(struct termios *t, speed_t speed)
{
    (void)t;
    (void)speed;
    return 0;
}

int cfsetospeed(struct termios *t, speed_t speed)
{
    (void)t;
    (void)speed;
    return 0;
}

int tcgetattr(int fd, struct termios *t)
{
    return ioctl(fd, TCGETS, t);
}

int tcsetattr(int fd, int optional_actions, const struct termios *t)
{
    (void)optional_actions;
    return ioctl(fd, TCSETS, (void *)t);
}

int tcflush(int fd, int queue_selector)
{
    (void)fd;
    (void)queue_selector;
    return 0;
}

int tcdrain(int fd)
{
    (void)fd;
    return 0;
}

pid_t tcgetpgrp(int fd)
{
    long r = __fantuan_raw(FANTUAN_SYS_TCGETPGRP, fd, 0, 0, 0, 0);
    if (r < 0 && r > -4096) {
        errno = (int)-r;
        return -1;
    }
    return (pid_t)r;
}

int tcsetpgrp(int fd, pid_t pgrp)
{
    long r = __fantuan_raw(FANTUAN_SYS_TCSETPGRP, fd, pgrp, 0, 0, 0);
    if (r < 0 && r > -4096) {
        errno = (int)-r;
        return -1;
    }
    return 0;
}
