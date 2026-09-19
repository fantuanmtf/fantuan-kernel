/* rump_dhcp.h - DHCP wire helpers shared by the R6 client (ours). */
#ifndef FANTUAN_RUMP_DHCP_H
#define FANTUAN_RUMP_DHCP_H

#include <sys/types.h>
#include <netinet/in.h>

#define DHCP_MSG_DISCOVER	1
#define DHCP_MSG_OFFER		2
#define DHCP_MSG_REQUEST	3
#define DHCP_MSG_ACK		5
#define DHCP_MSG_NAK		6

struct dhcp_lease {
	struct in_addr addr, mask, gw, dns, server;
	uint8_t type;
};

struct sockaddr_in;
struct sockaddr_in *dhcp_sin(struct sockaddr_in *, uint32_t, uint16_t);
int dhcp_if_provisional(void);
int dhcp_if_apply(const struct dhcp_lease *);

/* Build a DISCOVER or REQUEST into out; returns the wire length. */
size_t dhcp_pkt_build(uint8_t, uint32_t, const uint8_t[6],
    const struct dhcp_lease *, void *, size_t);
/* Parse a reply for the given xid; 1 with out filled, 0 when not ours. */
int dhcp_pkt_parse(const void *, size_t, uint32_t, const uint8_t[6],
    struct dhcp_lease *);

#endif
