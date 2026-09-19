/* rump_dhcp_pkt.c - DHCP BOOTP wire format (ours). */

#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include "rump_dhcp.h"

#define DHCP_MAGIC	0x63825363u

#define OPT_MSG_TYPE	53
#define OPT_MASK	1
#define OPT_ROUTER	3
#define OPT_DNS		6
#define OPT_REQ_IP	50
#define OPT_SERVER_ID	54

struct dhcp_pkt {
	uint8_t op, htype, hlen, hops;
	uint32_t xid;
	uint16_t secs, flags;
	struct in_addr ciaddr, yiaddr, siaddr, giaddr;
	uint8_t chaddr[16];
	uint8_t sname[64];
	uint8_t file[128];
	uint32_t magic;
	uint8_t options[312];
};

size_t
dhcp_pkt_build(uint8_t type, uint32_t xid, const uint8_t mac[6],
    const struct dhcp_lease *req, void *out, size_t max)
{
	struct dhcp_pkt *p = out;
	size_t n = 0;

	if (max < sizeof(*p))
		return 0;
	memset(p, 0, sizeof(*p));
	p->op = 1;		/* BOOTREQUEST */
	p->htype = 1;		/* Ethernet */
	p->hlen = 6;
	p->xid = xid;
	p->flags = htons(0x8000);	/* broadcast reply please */
	memcpy(p->chaddr, mac, 6);
	p->magic = htonl(DHCP_MAGIC);
	p->options[n++] = OPT_MSG_TYPE;
	p->options[n++] = 1;
	p->options[n++] = type;
	if (type == DHCP_MSG_REQUEST) {
		p->options[n++] = OPT_REQ_IP;
		p->options[n++] = 4;
		memcpy(&p->options[n], &req->addr, 4);
		n += 4;
		p->options[n++] = OPT_SERVER_ID;
		p->options[n++] = 4;
		memcpy(&p->options[n], &req->server, 4);
		n += 4;
	}
	p->options[n++] = 55;	/* parameter request list */
	p->options[n++] = 3;
	p->options[n++] = OPT_MASK;
	p->options[n++] = OPT_ROUTER;
	p->options[n++] = OPT_DNS;
	p->options[n++] = 255;
	return 240 + n;
}

int
dhcp_pkt_parse(const void *buf, size_t len, uint32_t xid,
    const uint8_t mac[6], struct dhcp_lease *out)
{
	const struct dhcp_pkt *p = buf;
	const uint8_t *o;
	size_t i, no;

	if (len < 240 || p->op != 2 || p->xid != xid)
		return 0;
	if (memcmp(p->chaddr, mac, 6) != 0)
		return 0;
	if (ntohl(p->magic) != DHCP_MAGIC)
		return 0;
	memset(out, 0, sizeof(*out));
	out->addr = p->yiaddr;
	out->server = p->siaddr;
	o = p->options;
	no = len - 240;
	i = 0;
	while (i < no) {
		uint8_t code = o[i++], olen;

		if (code == 0)
			continue;
		if (code == 255 || i >= no)
			break;
		olen = o[i++];
		if (i + olen > no)
			break;
		switch (code) {
		case OPT_MSG_TYPE:
			if (olen >= 1)
				out->type = o[i];
			break;
		case OPT_MASK:
			if (olen >= 4)
				memcpy(&out->mask, o + i, 4);
			break;
		case OPT_ROUTER:
			if (olen >= 4)
				memcpy(&out->gw, o + i, 4);
			break;
		case OPT_DNS:
			if (olen >= 4)
				memcpy(&out->dns, o + i, 4);
			break;
		case OPT_SERVER_ID:
			if (olen >= 4)
				memcpy(&out->server, o + i, 4);
			break;
		}
		i += olen;
	}
	return out->type != 0 ? 1 : 0;
}
