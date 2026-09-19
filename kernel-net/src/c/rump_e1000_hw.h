/* rump_e1000_hw.h - softc + register access shared by the e1000 split
 * (ours).  The device is a single static instance. */
#ifndef FANTUAN_RUMP_E1000_HW_H
#define FANTUAN_RUMP_E1000_HW_H

#include <sys/types.h>
#include "rump_e1000_reg.h"

struct e1000_softc {
	volatile uint8_t *regs;
	struct e1000_rx_desc *rx_ring;
	struct e1000_tx_desc *tx_ring;
	uint8_t *rx_bufs;
	uint8_t *tx_bufs;
	uint64_t rx_ring_phys, tx_ring_phys, rx_bufs_phys, tx_bufs_phys;
	uint8_t mac[6];
	unsigned rx_cur;
	unsigned tx_cur, tx_clean, tx_used;
	unsigned pkts_in, pkts_out;
	int ok;
};

extern struct e1000_softc e1000_sc;

static inline uint32_t
e1000_rd(uint32_t off)
{

	return *(volatile uint32_t *)(e1000_sc.regs + off);
}

static inline void
e1000_wr(uint32_t off, uint32_t val)
{

	*(volatile uint32_t *)(e1000_sc.regs + off) = val;
}

#endif
