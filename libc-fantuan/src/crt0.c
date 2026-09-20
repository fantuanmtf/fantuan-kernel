/* libc-fantuan — process startup (P1).
 *
 * The kernel passes argc/argv/envp on the stack; environ points at the
 * kernel-provided environment block (empty in P1, since SYS_EXEC has no
 * envp argument yet). exit() flushes stdio and calls _exit. */
#include <stdlib.h>
#include <unistd.h>
#include <stdio.h>

char **environ;
int errno;

int main(int argc, char **argv);

void __libc_start_main(long argc, char **argv, char **envp)
{
    environ = envp ? envp : (argv + argc + 1);
    exit(main((int)argc, argv));
}

/* libc-internal: __errno location (single-threaded; threads are out of
 * scope until the P2/P3 batches). */
int *__errno_location(void)
{
    return &errno;
}
