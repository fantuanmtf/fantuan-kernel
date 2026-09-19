/* fantuan adaptation layer for the NetBSD rump slice (M11 R2-R4).
 * Internal interface between the adapter C files and the Rust half of
 * kernel-net; ours, not upstream. */
#ifndef FANTUAN_RUMP_SHIM_H
#define FANTUAN_RUMP_SHIM_H

#include <sys/types.h>

struct ifnet;
struct mbuf;

/* Driver-facing interface registry (docs/M11_NET.md section 6).  Drivers
 * register a table; net_ifattach() binds it to a rump ifnet. */
struct net_ops {
	const char *name;
	int (*init)(void *priv);
	void (*mac)(void *priv, uint8_t out[6]);
	int (*send)(void *priv, const void *frame, size_t len);
	/* Returns >=0 length, 0 when nothing is pending, -1 on error. */
	int (*recv)(void *priv, void *frame, size_t max);
	int (*link)(void *priv);
};

int net_register(const struct net_ops *, void *);
int net_ifattach(const struct net_ops *, struct ifnet *);
void net_ifdetach(const struct net_ops *);
int net_send(const void *, size_t);
int net_recv(void *, size_t);

/* pktqueue(9) replacement (rump_shim_net.c): drained from the net task. */
void rump_pktq_drain(void);

/* Real IPv4 bring-up and the boot test state machine (rump_ip4.c). */
void rump_ip4_up(void);
int rump_net_poll(void);

/* Loopback lo0 bring-up (rump_loopback.c). */
void rump_loopback_up(void);
struct ifnet *rump_loopback_ifp(void);
int rump_loopback_ready(void);

/* ICMP echo client over ip_output (rump_ping.c). */
void rump_ping_begin(void);
int rump_ping_poll(void);
int rump_ping_rx(struct mbuf *);

/* UDP loopback exchange over the real PCB/udp_input path (rump_udp.c). */
int rump_udp_run(void);

/* ARP self-test on the shim ethernet interface (rump_arp.c). */
int rump_arp_up(void);
void rump_arp_test_start(void);
int rump_arp_test_poll(void);
int rump_arp_entries(void);

void fantuan_rump_log(const void *, size_t);
void fantuan_rump_panic(const void *, size_t) __attribute__((noreturn));
void *fantuan_rump_pages_alloc(size_t);
void fantuan_rump_pages_free(void *, size_t);
uint64_t fantuan_rump_ticks(void);
uint64_t fantuan_rump_physmem_pages(void);
uint64_t fantuan_rump_switch_count(void);

void rump_shim_init(void);
void rump_shim_tick(void);
void rump_softint_dispatch(void);
int rump_selftest_poll(void);

#endif
