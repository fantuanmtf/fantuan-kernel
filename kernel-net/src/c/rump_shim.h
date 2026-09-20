/* fantuan adaptation layer for the NetBSD rump slice (M11 R2-R4).
 * Internal interface between the adapter C files and the Rust half of
 * kernel-net; ours, not upstream. */
#ifndef FANTUAN_RUMP_SHIM_H
#define FANTUAN_RUMP_SHIM_H

#include <sys/types.h>
#include <stdint.h>

struct ifnet;
struct mbuf;
struct socket;
struct lwp;

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

/* Deferred workqueue drain (rump_shim_if.c): runs in the softintd task. */
void rump_workqueue_drain(void);

/* Real IPv4 bring-up and the boot test state machine (rump_ip4.c). */
void rump_ip4_up(void);
int rump_net_poll(void);

/* Loopback lo0 bring-up (rump_loopback.c). */
void rump_loopback_up(void);
struct ifnet *rump_loopback_ifp(void);
int rump_loopback_ready(void);

/* ICMP echo client over ip_output (rump_ping.c). */
void rump_ping_begin(void);
void rump_ping_begin_addr(uint32_t);
int rump_ping_poll(void);
int rump_ping_rx(struct mbuf *);
int rump_ping_rtt(void);
int rump_ping_seq(void);
const char *rump_ping_error(void);

/* UDP loopback exchange over the real PCB/udp_input path (rump_udp.c). */
int rump_udp_run(void);

/* Real socket/TCP loopback tests (rump_tcp.c): 0 running, 1 done, -1 fail. */
void rump_tcp_begin(void);
int rump_tcp_poll(void);

/* DHCP client over the real UDP/socket layer (rump_dhcp.c): 0 running,
 * 1 done, -1 failed (the marker is printed internally). */
void rump_dhcp_begin(void);
int rump_dhcp_poll(void);
/* Lease DNS server in network byte order (R7 resolver input). */
uint32_t rump_dhcp_dns(void);

/* Offline HTTP GET over the real TCP socket layer (rump_http.c). */
void rump_http_begin(void);
void rump_http_begin_wget(const char *, size_t, uint32_t, uint16_t,
    uint16_t, int);
int rump_http_poll(void);
int rump_http_status_code(void);
size_t rump_http_body_len(void);
uint32_t rump_http_body_hash(void);
const char *rump_http_error(void);

/* DNS A-query client over the real UDP socket layer (rump_dns.c). */
void rump_dns_start(const char *, size_t, uint32_t, uint16_t);
int rump_dns_poll(void);
uint32_t rump_dns_result(void);
uint32_t rump_dns_default_server(void);
const char *rump_dns_error(void);

/* R7 boot self-test sequence (rump_tools.c): 0 running, 1 done, -1 failed. */
void rump_tools_begin(void);
int rump_tools_poll(void);

/* R8 boot sequence (rump_r8.c): HTTPS over the pinned CA, the host UDP echo
 * test and the optional external phase; 1 when finished (never fails). */
void rump_r8_begin(void);
int rump_r8_poll(void);
/* Build-time offline-fixture switch (FANTUAN_NET_FIXTURES=1 in the smoke). */
int rump_net_fixtures(void);

/* R7 shell tool request slot (rump_toolreq.c): the net task runs the
 * client, the shell polls the status.  One request at a time. */
void rump_tool_begin_dns(const char *, size_t, uint32_t, uint16_t);
void rump_tool_begin_ping(uint32_t);
void rump_tool_begin_wget(const char *, size_t, uint32_t, uint16_t, uint16_t);
int rump_tool_poll(void);
int rump_tool_status(void);
const char *rump_tool_error(void);
uint32_t rump_tool_result_addr(void);
int rump_tool_result_rtt(void);
int rump_tool_result_http(void);
size_t rump_tool_result_bytes(void);
uint32_t rump_tool_result_hash(void);

/* 1 while the boot self-test sequence owns the shared clients. */
int rump_net_selftest_busy(void);

/* Connection helpers for the TCP test (rump_tcp_conn.c). */
int rump_tcp_pair(uint16_t, uint16_t, struct socket **, struct socket **,
    struct socket **, struct lwp *);
int rump_tcp_connected(struct socket *);
int rump_tcp_accept(struct socket *, struct socket **);
void rump_tcp_shutdown(struct socket *, struct socket *, int *, int *);
void rump_tcp_close(struct socket **, struct socket **, struct socket **);

/* Payload/hash/non-blocking I/O for the TCP test (rump_tcp_io.c). */
void rump_tcp_io_init(void);
void rump_tcp_io_reset(void);
int rump_tcp_send(struct socket *, struct lwp *);
int rump_tcp_recv(struct socket *, size_t *);
int rump_tcp_verify(size_t);
uint32_t rump_tcp_hash(const uint8_t *, size_t);
uint32_t rump_tcp_rx_hash(size_t);

/* Deterministic loopback loss injection (rump_loss.c, pktq_enqueue hook). */
void rump_loss_arm(uint16_t sport, uint16_t dport, int ndrops);
void rump_loss_disarm(void);
unsigned rump_loss_dropped(void);
unsigned rump_loss_retrans(void);
struct pktqueue;
bool rump_loss_drop_if_armed(struct pktqueue *, struct mbuf *);

/* ARP self-test on the shim ethernet interface (rump_arp.c). */
int rump_arp_up(void);
void rump_arp_test_start(void);
int rump_arp_test_poll(void);
int rump_arp_entries(void);

void fantuan_rump_yield(void);
void fantuan_rump_log(const void *, size_t);
void fantuan_rump_panic(const void *, size_t) __attribute__((noreturn));
void *fantuan_rump_pages_alloc(size_t);
void fantuan_rump_pages_free(void *, size_t);
uint64_t fantuan_rump_ticks(void);
uint64_t fantuan_rump_physmem_pages(void);
uint64_t fantuan_rump_switch_count(void);
uint32_t fantuan_rump_pci_read(uint8_t, uint8_t, uint8_t, uint8_t);
void fantuan_rump_pci_write(uint8_t, uint8_t, uint8_t, uint8_t, uint32_t);
void *fantuan_rump_mmio_map(uint64_t, uint64_t);
uint64_t fantuan_rump_virt_to_phys(const void *);

void rump_shim_init(void);
void rump_shim_tick(void);
void rump_softint_dispatch(void);
int rump_selftest_poll(void);

#endif
