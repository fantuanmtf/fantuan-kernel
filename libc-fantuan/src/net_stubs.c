/* libc-fantuan — socket stubs (P3): sys/socket.h exists so configure and
 * bash compile, but there is no network stack at this ABI layer (the
 * kernel's tool bridge owns the NIC). Every call fails ENOSYS. */
#include <errno.h>
#include <sys/socket.h>

int socket(int domain, int type, int protocol)
{
    (void)domain;
    (void)type;
    (void)protocol;
    errno = ENOSYS;
    return -1;
}

int socketpair(int domain, int type, int protocol, int sv[2])
{
    (void)domain;
    (void)type;
    (void)protocol;
    (void)sv;
    errno = ENOSYS;
    return -1;
}

int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen)
{
    (void)sockfd;
    (void)addr;
    (void)addrlen;
    errno = ENOSYS;
    return -1;
}

int connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen)
{
    (void)sockfd;
    (void)addr;
    (void)addrlen;
    errno = ENOSYS;
    return -1;
}

int listen(int sockfd, int backlog)
{
    (void)sockfd;
    (void)backlog;
    errno = ENOSYS;
    return -1;
}

int accept(int sockfd, struct sockaddr *addr, socklen_t *addrlen)
{
    (void)sockfd;
    (void)addr;
    (void)addrlen;
    errno = ENOSYS;
    return -1;
}

int getsockname(int sockfd, struct sockaddr *addr, socklen_t *addrlen)
{
    (void)sockfd;
    (void)addr;
    (void)addrlen;
    errno = ENOSYS;
    return -1;
}

int getpeername(int sockfd, struct sockaddr *addr, socklen_t *addrlen)
{
    (void)sockfd;
    (void)addr;
    (void)addrlen;
    errno = ENOSYS;
    return -1;
}

int shutdown(int sockfd, int how)
{
    (void)sockfd;
    (void)how;
    errno = ENOSYS;
    return -1;
}

int setsockopt(int sockfd, int level, int optname, const void *optval, socklen_t optlen)
{
    (void)sockfd;
    (void)level;
    (void)optname;
    (void)optval;
    (void)optlen;
    errno = ENOSYS;
    return -1;
}

int getsockopt(int sockfd, int level, int optname, void *optval, socklen_t *optlen)
{
    (void)sockfd;
    (void)level;
    (void)optname;
    (void)optval;
    (void)optlen;
    errno = ENOSYS;
    return -1;
}

ssize_t send(int sockfd, const void *buf, size_t len, int flags)
{
    (void)sockfd;
    (void)buf;
    (void)len;
    (void)flags;
    errno = ENOSYS;
    return -1;
}

ssize_t recv(int sockfd, void *buf, size_t len, int flags)
{
    (void)sockfd;
    (void)buf;
    (void)len;
    (void)flags;
    errno = ENOSYS;
    return -1;
}

ssize_t sendto(int sockfd, const void *buf, size_t len, int flags,
               const struct sockaddr *dest_addr, socklen_t addrlen)
{
    (void)sockfd;
    (void)buf;
    (void)len;
    (void)flags;
    (void)dest_addr;
    (void)addrlen;
    errno = ENOSYS;
    return -1;
}

ssize_t recvfrom(int sockfd, void *buf, size_t len, int flags,
                 struct sockaddr *src_addr, socklen_t *addrlen)
{
    (void)sockfd;
    (void)buf;
    (void)len;
    (void)flags;
    (void)src_addr;
    (void)addrlen;
    errno = ENOSYS;
    return -1;
}
