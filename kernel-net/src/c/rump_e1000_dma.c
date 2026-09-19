/* rump_e1000_dma.c - e1000 MAC/EEPROM and the polled TX/RX paths (ours).
 * The descriptor memory lives in normal RAM (x86_64 DMA is coherent), so
 * a compiler barrier before ringing the doorbells is enough.  TX reclaims
 * completed descriptors on every call; RX gives the processed descriptor
 * back by advancing RDT.  All steps are bounded. */

#include <sys/types.h>
#include <sys/param.h>
#include <sys/systm.h>
#include <sys/atomic.h>
#include <net/if_ether.h>
#include <netinet/in.h>
#include "rump_shim.h"
#include "rump_e1000.h"
#include "rump_e1000_hw.h"

static uint16_t
e1000_eeprom_read(uint8_t addr)
{
	uint32_t v;
	int i;

	e1000_wr(E1000_EERD, ((uint32_t)addr << 8) | E1000_EERD_START);
	for (i = 0; i < 100000; i++) {
		v = e1000_rd(E1000_EERD);
		if (v & E1000_EERD_DONE)
			return (uint16_t)(v >> 16);
		if ((v & E1000_EERD_START) == 0)
			break;
	}
	return 0;
}

void
e1000_hw_read_mac(void)
{
	uint32_t ral, rah;
	uint16_t w;
	int i;

	ral = e1000_rd(E1000_RAL);
	rah = e1000_rd(E1000_RAH);
	e1000_sc.mac[0] = (uint8_t)ral;
	e1000_sc.mac[1] = (uint8_t)(ral >> 8);
	e1000_sc.mac[2] = (uint8_t)(ral >> 16);
	e1000_sc.mac[3] = (uint8_t)(ral >> 24);
	e1000_sc.mac[4] = (uint8_t)rah;
	e1000_sc.mac[5] = (uint8_t)(rah >> 8);
	for (i = 0; i < 6; i++)
		if (e1000_sc.mac[i] != 0)
			return;
	/* QEMU loads RA[0] at reset; the EEPROM path is the fallback. */
	for (i = 0; i < 3; i++) {
		w = e1000_eeprom_read((uint8_t)i);
		e1000_sc.mac[i * 2] = (uint8_t)w;
		e1000_sc.mac[i * 2 + 1] = (uint8_t)(w >> 8);
	}
	ral = (uint32_t)e1000_sc.mac[0] | (e1000_sc.mac[1] << 8) |
	    (e1000_sc.mac[2] << 16) | ((uint32_t)e1000_sc.mac[3] << 24);
	rah = (uint32_t)e1000_sc.mac[4] | (e1000_sc.mac[5] << 8);
	e1000_wr(E1000_RAL, ral);
	e1000_wr(E1000_RAH, rah | (1u << 31));
}

/* Software checksum completion for IPv4 TCP/UDP frames.  The stack leaves
 * the pseudo-header partial in the checksum field for hardware offload; the
 * frame is linear here, so finish it with a plain one's-complement sum
 * before the descriptor is handed to the device.  A no-op for ARP/other. */
static uint16_t
e1000_sum(const uint8_t *p, size_t len, uint32_t sum)
{
	size_t i;

	for (i = 0; i + 1 < len; i += 2)
		sum += ((uint32_t)p[i] << 8) | p[i + 1];
	if (i < len)
		sum += (uint32_t)p[i] << 8;
	while (sum >> 16)
		sum = (sum & 0xffff) + (sum >> 16);
	return (uint16_t)sum;
}

static void
e1000_fix_csum(uint8_t *frame, size_t len)
{
	uint8_t *ip, *l4, *field;
	uint16_t csum;
	uint32_t l4len;
	int hlen, proto;

	if (len < ETHER_HDR_LEN + 20)
		return;
	ip = frame + ETHER_HDR_LEN;
	if ((ip[0] >> 4) != 4)
		return;
	proto = ip[9];
	if (proto != IPPROTO_TCP && proto != IPPROTO_UDP)
		return;
	hlen = (ip[0] & 0xf) << 2;
	l4len = ((uint32_t)ip[2] << 8 | ip[3]) - (uint32_t)hlen;
	if (hlen < 20 || ETHER_HDR_LEN + hlen + l4len > len)
		return;
	l4 = ip + hlen;
	field = l4 + (proto == IPPROTO_UDP ? 6 : 16);
	field[0] = 0;
	field[1] = 0;
	csum = e1000_sum(l4, l4len, 0);
	csum = e1000_sum(ip + 12, 8, csum);	/* pseudo-header src+dst */
	csum += (uint16_t)proto;
	csum += (uint16_t)l4len;
	while (csum >> 16)
		csum = (uint16_t)((csum & 0xffff) + (csum >> 16));
	csum = (uint16_t)~csum;
	if (csum == 0 && proto == IPPROTO_UDP)
		csum = 0xffff;
	field[0] = (uint8_t)(csum >> 8);
	field[1] = (uint8_t)csum;
}

void
e1000_hw_tx_reclaim(void)
{
	struct e1000_tx_desc *d;

	while (e1000_sc.tx_used > 0) {
		d = &e1000_sc.tx_ring[e1000_sc.tx_clean];
		if ((d->status & E1000_TXD_STAT_DD) == 0)
			break;
		d->status = 0;
		e1000_sc.tx_clean = (e1000_sc.tx_clean + 1) % E1000_TX_DESC;
		e1000_sc.tx_used--;
	}
}

int
e1000_hw_send(const void *frame, size_t len)
{
	struct e1000_tx_desc *d;
	int spin;

	if (!e1000_sc.ok || len == 0 || len > E1000_BUF_SIZE)
		return -1;
	e1000_hw_tx_reclaim();
	if (e1000_sc.tx_used >= E1000_TX_DESC) {
		for (spin = 0; spin < 200000; spin++) {
			e1000_hw_tx_reclaim();
			if (e1000_sc.tx_used < E1000_TX_DESC)
				break;
		}
		if (e1000_sc.tx_used >= E1000_TX_DESC)
			return -1;
	}
	d = &e1000_sc.tx_ring[e1000_sc.tx_cur];
	memcpy(e1000_sc.tx_bufs + e1000_sc.tx_cur * E1000_BUF_SIZE, frame,
	    len);
	e1000_fix_csum(e1000_sc.tx_bufs + e1000_sc.tx_cur * E1000_BUF_SIZE,
	    len);
	d->addr = e1000_sc.tx_bufs_phys +
	    (uint64_t)e1000_sc.tx_cur * E1000_BUF_SIZE;
	d->length = (uint16_t)len;
	d->cso = 0;
	d->css = 0;
	d->special = 0;
	d->cmd = E1000_TXD_CMD_EOP | E1000_TXD_CMD_IFCS | E1000_TXD_CMD_RS;
	d->status = 0;
	e1000_sc.tx_cur = (e1000_sc.tx_cur + 1) % E1000_TX_DESC;
	e1000_sc.tx_used++;
	membar_producer();
	e1000_wr(E1000_TDT, e1000_sc.tx_cur);
	e1000_sc.pkts_out++;
	return (int)len;
}

int
e1000_hw_recv(void *frame, size_t max)
{
	struct e1000_rx_desc *d;
	int len;

	if (!e1000_sc.ok)
		return 0;
	d = &e1000_sc.rx_ring[e1000_sc.rx_cur];
	if ((d->status & E1000_RXD_STAT_DD) == 0)
		return 0;
	len = d->length;
	if (len > (int)max)
		len = (int)max;
	memcpy(frame, e1000_sc.rx_bufs + e1000_sc.rx_cur * E1000_BUF_SIZE,
	    (size_t)len);
	d->status = 0;
	membar_producer();
	e1000_wr(E1000_RDT, e1000_sc.rx_cur);
	e1000_sc.rx_cur = (e1000_sc.rx_cur + 1) % E1000_RX_DESC;
	e1000_sc.pkts_in++;
	return len;
}

int
e1000_hw_link(void)
{

	if (!e1000_sc.ok)
		return 0;
	return (e1000_rd(E1000_STATUS) & E1000_STATUS_LU) != 0;
}

void
e1000_hw_mac(uint8_t out[6])
{

	memcpy(out, e1000_sc.mac, 6);
}
