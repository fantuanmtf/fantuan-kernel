/* rump_nic.h - the single NIC contract of the running port (ours, M11 R9b).
 * The driver behind it is chosen at compile time: e1000 (x86_64 PCI) or
 * virtio-net (aarch64 MMIO).  rump_ip4.c, rump_dhcp*.c and the counters use
 * only this interface, so the stack code stays arch-neutral. */
#ifndef FANTUAN_RUMP_NIC_H
#define FANTUAN_RUMP_NIC_H

#include <sys/types.h>

struct ifnet;

/* Probe/init the NIC and attach its ifnet; 0 ok, ENXIO no device. */
int rump_nic_up(void);
/* Drain the polled RX ring and reclaim TX descriptors. */
void rump_nic_poll(void);
int rump_nic_ready(void);
struct ifnet *rump_nic_ifp(void);
/* Read the hardware MAC address. */
void rump_nic_mac(uint8_t out[6]);
/* Driver name for the boot markers and the DHCP ifra_name. */
const char *rump_nic_name(void);
void rump_nic_counters(unsigned long long *in, unsigned long long *out);

#endif /* FANTUAN_RUMP_NIC_H */
