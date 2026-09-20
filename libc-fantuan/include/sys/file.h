/* libc-fantuan — sys/file.h (P1): flock is a stub until P2. */
#ifndef _SYS_FILE_H
#define _SYS_FILE_H

#define LOCK_SH 1
#define LOCK_EX 2
#define LOCK_NB 4
#define LOCK_UN 8

int flock(int fd, int operation);

#endif /* _SYS_FILE_H */
