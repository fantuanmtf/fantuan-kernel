/* rump_udp_host.h - host UDP echo test interface (ours, M11 R8). */
#ifndef FANTUAN_RUMP_UDP_HOST_H
#define FANTUAN_RUMP_UDP_HOST_H

#include <sys/types.h>

/* Send/verify N datagrams against the SLIRP host fixture; 0 on success. */
int rump_udp_host_run(void);
int rump_udp_host_tx(void);
int rump_udp_host_rx(void);
size_t rump_udp_host_bytes(void);

#endif
