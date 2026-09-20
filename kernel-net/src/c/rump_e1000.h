/* rump_e1000.h - internal split of the M11 R6 e1000 adapter (ours).
 * rump_e1000.c owns the PCI/MMIO setup, rump_e1000_dma.c the rings and
 * send/receive paths, rump_e1000_if.c binds the hardware to the shared
 * rump_ether_if core (rump_ether_if.h). */
#ifndef FANTUAN_RUMP_E1000_H
#define FANTUAN_RUMP_E1000_H

#include <sys/types.h>

#define E1000_RX_DESC	32
#define E1000_TX_DESC	32
#define E1000_BUF_SIZE	2048

/* Hardware side (rump_e1000.c/rump_e1000_dma.c); errno or length. */
int e1000_hw_init(void);
int e1000_hw_send(const void *, size_t);
int e1000_hw_recv(void *, size_t);
int e1000_hw_link(void);
void e1000_hw_tx_reclaim(void);
void e1000_hw_mac(uint8_t out[6]);

#endif /* FANTUAN_RUMP_E1000_H */
