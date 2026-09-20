/* rump_dns.h - DNS A-query client interface (ours, M11 R7).
 * The wire format lives in rump_dns_pkt.c, the UDP state machine in
 * rump_dns.c; both are adapter code, not imported NetBSD source. */
#ifndef FANTUAN_RUMP_DNS_H
#define FANTUAN_RUMP_DNS_H

#include <sys/types.h>

/* dns_pkt_parse() result codes (0 is success). */
#define DNS_PKT_OK		0
#define DNS_PKT_SHORT		-1
#define DNS_PKT_ID		-2
#define DNS_PKT_RCODE		-3
#define DNS_PKT_QUESTION	-4
#define DNS_PKT_FORMAT		-5
#define DNS_PKT_NOADDR		-6

/* Build a standard recursive A query for NAME; returns the wire length
 * or 0 when the name/buffer is unusable. */
size_t dns_pkt_build(const char *, size_t, uint16_t, void *, size_t);
/* Validate ID/question echo and extract the first A record address
 * (host byte order); returns a DNS_PKT_* code. */
int dns_pkt_parse(const void *, size_t, uint16_t, const char *, size_t,
    uint32_t *);

#endif
