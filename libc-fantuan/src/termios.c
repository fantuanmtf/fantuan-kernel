/* libc-fantuan — termios (P1): the serial console reports defaults and
 * accepts sets (no line discipline). Process-group calls arrive in P2. */
#include <errno.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>

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
    (void)fd;
    errno = ENOSYS;
    return -1;
}

int tcsetpgrp(int fd, pid_t pgrp)
{
    (void)fd;
    (void)pgrp;
    errno = ENOSYS;
    return -1;
}
