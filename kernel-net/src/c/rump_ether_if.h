/* rump_ether_if.h - shared ethernet ifnet core for the NIC drivers (ours,
 * M11 R9b).  e1000 and virtio-net differ only in these six hardware
 * operations; the ifnet setup, ether_output counterpart, RX demux and the
 * net_ops table live once in rump_ether_if.c. */
#ifndef FANTUAN_RUMP_ETHER_IF_H
#define FANTUAN_RUMP_ETHER_IF_H

#include <sys/types.h>

struct ifnet;
struct net_ops;

/* One polled receive buffer fits an Ethernet frame plus slack. */
#define RUMP_ETHER_BUF_SIZE 2048

struct rump_ether_hw {
	/* 0 ok, ENXIO no device, other errno failure. */
	int (*init)(void);
	void (*mac)(uint8_t out[6]);
	int (*send)(const void *frame, size_t len);
	/* >=0 length, 0 nothing pending, -1 error. */
	int (*recv)(void *frame, size_t max);
	void (*tx_reclaim)(void);
	int (*link)(void);
};

int rump_ether_up(const char *name, const struct rump_ether_hw *hw);
/* The registered ops table (rump_ether_ops.c) and the active hardware. */
const struct net_ops *rump_ether_ops(void);
const struct rump_ether_hw *rump_ether_hw(void);
void rump_ether_poll(void);
int rump_ether_ready(void);
struct ifnet *rump_ether_ifp(void);
void rump_ether_mac(uint8_t out[6]);
void rump_ether_counters(unsigned long long *in, unsigned long long *out);

#endif /* FANTUAN_RUMP_ETHER_IF_H */
