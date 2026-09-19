/* fantuan adaptation shim: sys/poll.h - poll event values for the socket
 * layer (ours, not upstream).  The poll(2) syscall layer is not part of the
 * M11 slice; the constants match NetBSD's sys/poll.h. */
#ifndef FANTUAN_SYS_POLL_H
#define FANTUAN_SYS_POLL_H

#define POLLIN		0x0001
#define POLLPRI		0x0002
#define POLLOUT		0x0004
#define POLLRDNORM	0x0040
#define POLLWRNORM	POLLOUT
#define POLLRDBAND	0x0080
#define POLLWRBAND	0x0100
#define POLLERR		0x0008
#define POLLHUP		0x0010
#define POLLNVAL	0x0020

/* Upstream declares this in kern_sig.c's private scope; the slice needs the
 * prototype for sowakeup()/sohasoutofband().  The adapter's implementation
 * drops the notification (no signal delivery before M14). */
void	fownsignal(int, int, int, int, void *);

#endif
