/* rump_virtio_net.h - virtio-net MMIO hardware interface (ours, M11 R9b).
 * rump_virtio_net.c implements these over the modern virtio 1.x MMIO
 * transport; rump_virtio_net_if.c binds them to the shared rump_ether_if
 * core and exports the arch-neutral rump_nic contract (rump_nic.h). */
#ifndef FANTUAN_RUMP_VIRTIO_NET_H
#define FANTUAN_RUMP_VIRTIO_NET_H

#include <sys/types.h>

/* Hardware side; errno or length. */
int vnet_hw_init(void);
void vnet_hw_mac(uint8_t out[6]);
int vnet_hw_send(const void *, size_t);
int vnet_hw_recv(void *, size_t);
void vnet_hw_tx_reclaim(void);
int vnet_hw_link(void);

#endif /* FANTUAN_RUMP_VIRTIO_NET_H */
