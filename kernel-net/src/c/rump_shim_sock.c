/* rump_shim_sock.c - sockaddr utility functions for the R3 loopback path
 * (ours).  NetBSD keeps these helpers in the socket layer (uipc_socket.c),
 * which is not part of the R3 import; the ifnet/link-address path needs a
 * correct subset.
 */
#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/kmem.h>
#include <sys/socket.h>
#include <sys/socketvar.h>
#include <sys/protosw.h>
#include <sys/domain.h>
#include <net/if.h>
#include <net/if_dl.h>
#include <netinet/in.h>
#include <netinet6/in6.h>
#include "rump_shim.h"

struct sockaddr *
sockaddr_copy(struct sockaddr *dst, socklen_t len, const struct sockaddr *src)
{

	if (src == NULL) {
		memset(dst, 0, len);
		return dst;
	}
	if (src->sa_len > len)
		return NULL;
	memcpy(dst, src, src->sa_len);
	if (src->sa_len < len)
		memset((char *)dst + src->sa_len, 0, len - src->sa_len);
	return dst;
}

struct sockaddr *
sockaddr_externalize(struct sockaddr *dst, socklen_t len,
    const struct sockaddr *src)
{

	return sockaddr_copy(dst, len, src);
}

int
sockaddr_cmp(const struct sockaddr *sa1, const struct sockaddr *sa2)
{
	const socklen_t len = MIN(sa1->sa_len, sa2->sa_len);
	int diff;

	diff = memcmp(sa1, sa2, len);
	if (diff != 0)
		return diff;
	return (int)sa1->sa_len - (int)sa2->sa_len;
}

static const struct sockaddr_in sockaddr_any_in = {
	.sin_len = sizeof(struct sockaddr_in),
	.sin_family = AF_INET,
};

const struct sockaddr *
sockaddr_any_by_family(sa_family_t family)
{

	if (family == AF_INET)
		return (const struct sockaddr *)&sockaddr_any_in;
	return NULL;
}

const struct sockaddr *
sockaddr_any(const struct sockaddr *sa)
{

	return sockaddr_any_by_family(sa->sa_family);
}

struct sockaddr *
sockaddr_dup(const struct sockaddr *sa, int flags)
{
	struct sockaddr *nsa;

	nsa = malloc(sa->sa_len, M_IFADDR, flags);
	if (nsa != NULL)
		memcpy(nsa, sa, sa->sa_len);
	return nsa;
}

void
sockaddr_free(struct sockaddr *sa)
{

	free(sa, M_IFADDR);
}

uint8_t
sockaddr_dl_measure(uint8_t namelen, uint8_t addrlen)
{
	size_t len = offsetof(struct sockaddr_dl, sdl_data) + namelen + addrlen;

	if (len > 255)
		len = 255;
	return (uint8_t)len;
}

struct sockaddr_dl *
sockaddr_dl_init(struct sockaddr_dl *sdl, socklen_t len, uint16_t index,
    uint8_t type, const void *name, uint8_t namelen, const void *addr,
    uint8_t addrlen)
{

	memset(sdl, 0, len);
	sdl->sdl_family = AF_LINK;
	sdl->sdl_len = len;
	sdl->sdl_index = index;
	sdl->sdl_type = type;
	sdl->sdl_nlen = namelen;
	sdl->sdl_alen = addrlen;
	if (name != NULL && namelen != 0)
		memcpy(sdl->sdl_data, name, namelen);
	if (addr != NULL && addrlen != 0)
		memcpy(LLADDR(sdl), addr, addrlen);
	return sdl;
}

struct sockaddr_dl *
sockaddr_dl_setaddr(struct sockaddr_dl *sdl, socklen_t len, const void *addr,
    uint8_t addrlen)
{

	if (sdl->sdl_nlen + addrlen > len - offsetof(struct sockaddr_dl, sdl_data))
		return NULL;
	sdl->sdl_alen = addrlen;
	memcpy(LLADDR(sdl), addr, addrlen);
	return sdl;
}
