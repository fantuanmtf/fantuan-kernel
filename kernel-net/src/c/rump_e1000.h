/* rump_e1000.h - internal split of the M11 R6 e1000 adapter (ours).
 * rump_e1000.c owns the PCI/MMIO/DMA hardware; rump_e1000_if.c owns the
 * ifnet, the net_ops table and the RX demux into the stack input queues. */
#ifndef FANTUAN_RUMP_E1000_H
#define FANTUAN_RUMP_E1000_H

#include <sys/types.h>

struct net_ops;

#define E1000_RX_DESC	32
#define E1000_TX_DESC	32
#define E1000_BUF_SIZE	2048

/* Hardware side (rump_e1000.c); errno or length, single device. */
int e1000_hw_init(void);
int e1000_hw_send(const void *, size_t);
int e1000_hw_recv(void *, size_t);
int e1000_hw_link(void);
void e1000_hw_tx_reclaim(void);
void e1000_hw_mac(uint8_t out[6]);

/* net_ops table (rump_e1000_ops.c). */
extern const struct net_ops e1000_net_ops;

/* ifnet side (rump_e1000_if.c). */
int rump_e1000_up(void);
void rump_e1000_poll(void);
int rump_e1000_ready(void);
struct ifnet *rump_e1000_ifp(void);
void rump_e1000_counters(unsigned long long *in, unsigned long long *out);

#endif
