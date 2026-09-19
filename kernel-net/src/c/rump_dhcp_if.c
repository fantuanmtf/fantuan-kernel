/* rump_dhcp_if.c - DHCP interface plumbing through the real control paths
 * (ours).  The provisional address and the lease are applied with
 * in_control(SIOCAIFADDR/SIOCDIFADDR); the host route to the server and
 * the default gateway are installed with rtrequest1(RTM_ADD). */

#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/ioctl.h>
#include <net/if.h>
#include <net/route.h>
#include <netinet/in.h>
#include <netinet/in_var.h>
#include "rump_shim.h"
#include "rump_dhcp.h"
#include "rump_e1000.h"

#define DHCP_IFNAME	"e1000"
#define PROV_ADDR	0xa9fe0101u	/* 169.254.1.1 */
#define PROV_MASK	0xffff0000u
#define LHOST_ADDR	0x0a000202u	/* 10.0.2.2 */

struct sockaddr_in *
dhcp_sin(struct sockaddr_in *sin, uint32_t addr, uint16_t port)
{

	memset(sin, 0, sizeof(*sin));
	sin->sin_len = sizeof(*sin);
	sin->sin_family = AF_INET;
	sin->sin_addr.s_addr = htonl(addr);
	sin->sin_port = htons(port);
	return sin;
}

static void
dhcp_alias(struct in_aliasreq *ifra)
{

	memset(ifra, 0, sizeof(*ifra));
	strlcpy(ifra->ifra_name, DHCP_IFNAME, sizeof(ifra->ifra_name));
	ifra->ifra_addr.sin_len = sizeof(ifra->ifra_addr);
	ifra->ifra_addr.sin_family = AF_INET;
	ifra->ifra_mask.sin_len = sizeof(ifra->ifra_mask);
	ifra->ifra_mask.sin_family = AF_INET;
}

int
dhcp_if_provisional(void)
{
	struct in_aliasreq ifra;
	struct sockaddr_in dst, ifa;
	struct rt_addrinfo info;

	if (rump_e1000_ifp() == NULL)
		return -1;
	dhcp_alias(&ifra);
	ifra.ifra_addr.sin_addr.s_addr = htonl(PROV_ADDR);
	ifra.ifra_mask.sin_addr.s_addr = htonl(PROV_MASK);
	if (in_control(NULL, SIOCAIFADDR, &ifra, rump_e1000_ifp()) != 0)
		return -1;
	/* Direct host route to the DHCP server through the e1000. */
	dhcp_sin(&dst, LHOST_ADDR, 0);
	dhcp_sin(&ifa, PROV_ADDR, 0);
	memset(&info, 0, sizeof(info));
	info.rti_info[RTAX_DST] = sintosa(&dst);
	info.rti_info[RTAX_IFA] = sintosa(&ifa);
	info.rti_flags = RTF_HOST | RTF_STATIC;
	info.rti_addrs = RTA_DST | RTA_IFA;
	if (rtrequest1(RTM_ADD, &info, NULL) != 0)
		return -1;
	return 0;
}

int
dhcp_if_apply(const struct dhcp_lease *lease)
{
	struct in_aliasreq ifra;
	struct sockaddr_in dst, gw, mask;
	struct rt_addrinfo info;

	dhcp_alias(&ifra);
	ifra.ifra_addr.sin_addr = lease->addr;
	ifra.ifra_mask.sin_addr = lease->mask;
	if (in_control(NULL, SIOCAIFADDR, &ifra, rump_e1000_ifp()) != 0)
		return -1;
	/* Drop the provisional address now that the lease is live. */
	dhcp_alias(&ifra);
	ifra.ifra_addr.sin_addr.s_addr = htonl(PROV_ADDR);
	(void)in_control(NULL, SIOCDIFADDR, &ifra, rump_e1000_ifp());

	dhcp_sin(&dst, INADDR_ANY, 0);
	dhcp_sin(&gw, ntohl(lease->gw.s_addr), 0);
	memset(&mask, 0, sizeof(mask));
	mask.sin_len = sizeof(mask);
	mask.sin_family = AF_INET;
	memset(&info, 0, sizeof(info));
	info.rti_info[RTAX_DST] = sintosa(&dst);
	info.rti_info[RTAX_GATEWAY] = sintosa(&gw);
	info.rti_info[RTAX_NETMASK] = sintosa(&mask);
	info.rti_flags = RTF_GATEWAY | RTF_STATIC;
	info.rti_addrs = RTA_DST | RTA_GATEWAY | RTA_NETMASK;
	if (rtrequest1(RTM_ADD, &info, NULL) != 0)
		return -1;
	return 0;
}
