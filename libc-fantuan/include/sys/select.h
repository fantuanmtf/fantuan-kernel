/* libc-fantuan — sys/select.h (P1): declarations only; poll/select land with
 * the P2 signals/process batch. */
#ifndef _SYS_SELECT_H
#define _SYS_SELECT_H

#include <sys/types.h>
#include <sys/time.h>

#define FD_SETSIZE 64
typedef struct {
    unsigned long __bits[FD_SETSIZE / (8 * sizeof(unsigned long))];
} fd_set;

#define FD_ZERO(set) __builtin_memset((set), 0, sizeof(fd_set))
#define FD_SET(fd, set) ((set)->__bits[(fd) / (8 * sizeof(unsigned long))] |= \
                         1UL << ((fd) % (8 * sizeof(unsigned long))))
#define FD_CLR(fd, set) ((set)->__bits[(fd) / (8 * sizeof(unsigned long))] &= \
                         ~(1UL << ((fd) % (8 * sizeof(unsigned long)))))
#define FD_ISSET(fd, set) (((set)->__bits[(fd) / (8 * sizeof(unsigned long))] >> \
                            ((fd) % (8 * sizeof(unsigned long)))) & 1UL)

int select(int nfds, fd_set *readfds, fd_set *writefds, fd_set *exceptfds,
           struct timeval *timeout);

#endif /* _SYS_SELECT_H */
