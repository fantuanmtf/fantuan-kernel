/* rump_dns_pkt.c - DNS query/response wire format for the R7 resolver
 * (ours).  Bounded, allocation-free parsing: the question echo is verified
 * label by label (case-insensitive, one compression pointer allowed) and
 * the first IN/A answer is returned.  No EDNS, no CNAME chasing: the
 * resolver only needs the A records of the offline gate and of a plain
 * DHCP resolver. */

#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include "rump_dns.h"

#define DNS_HDR		12
#define DNS_TYPE_A	1
#define DNS_CLASS_IN	1
#define DNS_FLAG_QR	0x8000
#define DNS_FLAG_RCODE	0x000f
#define DNS_NAME_MAX	255
#define DNS_LABEL_MAX	63
#define DNS_JUMPS_MAX	16

static uint16_t
dns_get16(const uint8_t *p)
{

	return (uint16_t)(((uint16_t)p[0] << 8) | p[1]);
}

size_t
dns_pkt_build(const char *name, size_t namelen, uint16_t id, void *out,
    size_t max)
{
	uint8_t *p = out;
	size_t n = DNS_HDR, i, lstart;

	if (namelen == 0 || namelen > DNS_NAME_MAX ||
	    max < DNS_HDR + namelen + 7)
		return 0;
	memset(p, 0, DNS_HDR);
	p[0] = (uint8_t)(id >> 8);
	p[1] = (uint8_t)id;
	p[2] = 0x01;			/* RD */
	p[5] = 0x01;			/* QDCOUNT = 1 */
	lstart = n++;			/* reserve the first length byte */
	for (i = 0; i < namelen; i++) {
		if (name[i] == '.') {
			if (n == lstart + 1 || n - lstart - 1 > DNS_LABEL_MAX)
				return 0;
			p[lstart] = (uint8_t)(n - lstart - 1);
			lstart = n++;
			continue;
		}
		p[n++] = (uint8_t)name[i];
		if (n - lstart > DNS_LABEL_MAX + 1)
			return 0;
	}
	if (n == lstart + 1 || n - lstart - 1 > DNS_LABEL_MAX)
		return 0;
	p[lstart] = (uint8_t)(n - lstart - 1);
	p[n++] = 0;			/* root label */
	p[n++] = 0;
	p[n++] = DNS_TYPE_A;
	p[n++] = 0;
	p[n++] = DNS_CLASS_IN;
	return n;
}

/* Case-insensitive comparison of one DNS label against NAME+OFF. */
static int
dns_label_eq(const uint8_t *p, size_t len, size_t off, const uint8_t *label,
    size_t llen)
{
	size_t i;

	if (off + llen > len)
		return 0;
	for (i = 0; i < llen; i++) {
		uint8_t a = p[off + i], b = label[i];

		if (a >= 'A' && a <= 'Z')
			a = (uint8_t)(a - 'A' + 'a');
		if (b >= 'A' && b <= 'Z')
			b = (uint8_t)(b - 'A' + 'a');
		if (a != b)
			return 0;
	}
	return 1;
}

/* Compare the (possibly compressed) name at OFF with NAME; returns the
 * offset just past the name in the record, or 0 on mismatch.  END is the
 * offset after the first pointer, which is what the record continues with. */
static size_t
dns_name_match(const uint8_t *p, size_t len, size_t off, const char *name,
    size_t namelen)
{
	size_t i = 0, end = 0, jumps = 0;

	while (off < len && jumps < DNS_JUMPS_MAX) {
		uint8_t l = p[off];

		if (l == 0) {
			if (i != namelen)
				return 0;
			return end != 0 ? end : off + 1;
		}
		if ((l & 0xc0) == 0xc0) {
			size_t ptr;

			if (off + 1 >= len)
				return 0;
			if (end == 0)
				end = off + 2;
			ptr = ((size_t)(l & 0x3f) << 8) | p[off + 1];
			if (ptr >= len)
				return 0;
			off = ptr;
			jumps++;
			continue;
		}
		if (l > DNS_LABEL_MAX || i + l > namelen ||
		    !dns_label_eq(p, len, off + 1, (const uint8_t *)name + i, l))
			return 0;
		i += l;
		off += 1 + l;
		if (i < namelen) {
			if (name[i] != '.')
				return 0;
			i++;
		}
	}
	return 0;
}

/* Skip the (possibly compressed) name at OFF; NEXT gets the offset the
 * record continues at. */
static int
dns_name_skip(const uint8_t *p, size_t len, size_t off, size_t *next)
{
	size_t jumps = 0;

	while (off < len && jumps < DNS_JUMPS_MAX) {
		uint8_t l = p[off];

		if (l == 0) {
			*next = off + 1;
			return 1;
		}
		if ((l & 0xc0) == 0xc0) {
			if (off + 1 >= len)
				return 0;
			*next = off + 2;
			return 1;
		}
		if (l > DNS_LABEL_MAX)
			return 0;
		off += 1 + l;
		jumps++;
	}
	return 0;
}

int
dns_pkt_parse(const void *buf, size_t len, uint16_t id, const char *name,
    size_t namelen, uint32_t *addr)
{
	const uint8_t *p = buf;
	size_t off, next;
	uint16_t flags;
	int qd, an, i;

	if (len < DNS_HDR)
		return DNS_PKT_SHORT;
	if (dns_get16(p) != id)
		return DNS_PKT_ID;
	flags = dns_get16(p + 2);
	if ((flags & DNS_FLAG_QR) == 0)
		return DNS_PKT_FORMAT;
	if ((flags & DNS_FLAG_RCODE) != 0)
		return DNS_PKT_RCODE;
	qd = dns_get16(p + 4);
	an = dns_get16(p + 6);
	off = DNS_HDR;
	for (i = 0; i < qd; i++) {
		off = dns_name_match(p, len, off, name, namelen);
		if (off == 0 || off + 4 > len)
			return DNS_PKT_QUESTION;
		if (dns_get16(p + off) != DNS_TYPE_A ||
		    dns_get16(p + off + 2) != DNS_CLASS_IN)
			return DNS_PKT_QUESTION;
		off += 4;
	}
	for (i = 0; i < an; i++) {
		uint16_t type, class_, rdlen;

		if (!dns_name_skip(p, len, off, &next) || next + 10 > len)
			return DNS_PKT_FORMAT;
		type = dns_get16(p + next);
		class_ = dns_get16(p + next + 2);
		rdlen = dns_get16(p + next + 8);
		off = next + 10;
		if (off + rdlen > len)
			return DNS_PKT_FORMAT;
		if (type == DNS_TYPE_A && class_ == DNS_CLASS_IN && rdlen == 4) {
			*addr = ((uint32_t)p[off] << 24) |
			    ((uint32_t)p[off + 1] << 16) |
			    ((uint32_t)p[off + 2] << 8) | p[off + 3];
			return DNS_PKT_OK;
		}
		off += rdlen;
	}
	return DNS_PKT_NOADDR;
}
