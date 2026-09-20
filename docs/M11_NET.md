# M11 Design — Network Layer (NetBSD-derived)

> Status: design of record for v0.0.3, written before code (DESIGN §13.5).
> Roadmap: `ROADMAP_v0.0.2+.md` §3. Licensing: `THIRD_PARTY.md`.

## 1. Goal

Bring up wired networking on x86_64 and arm64 with a full TCP/IP stack, so
the Live kernel can use the network: `ping`, DHCP, DNS and HTTP/HTTPS
downloads in v0.0.3, and sockets for userspace in M14.

## 2. Source and licensing

The stack is **borrowed from NetBSD** (DESIGN §2 already names the
NetBSD/rump stack as the intended migration source), because:

- NetBSD's `sys/net`/`sys/netinet` is BSD-2/BSD-3 — compatible with this
  project's BSD-3-Clause license (Linux `net/` is GPLv2 and cannot be
  imported);
- it is C, matching the project's C driver layer;
- **rump kernels** are the proven method for running those sources outside
  a full NetBSD kernel: the code is designed to be separated from the host.

Imported sources live under `third_party/netbsd/` with their upstream
headers and a per-file `ORIGIN` note; they are exempt from the 300-line
rule (our wrappers are not). `THIRD_PARTY.md` is updated with versions,
commits and modifications when the import lands. `sys/net`'s
`SPDX`-visible BSD headers are preserved verbatim.

## 3. Architecture

```
tools (ping/wget/nslookup)      user sockets (M14 syscalls)
        \                         /
         kernel socket API (sosend/soreceive wrappers)
                    |
        NetBSD stack: socket -> tcp/udp -> ip -> ether/arp
                    |
              if.c / if_ethersubr
                    |
        net_ops registry (drivers/c/net.c)  <-> pool(DMA) via k_alloc_page
                    |
        virtio-net (MMIO + PCI) | e1000
```

- **Drivers** register a `net_ops` table exactly like `blk_ops`:
  `name/init/mac/send/recv/link_status`. `net.c` owns the registry and the
  first-device default, mirroring `blk.c`.
- **Polling first**: the interface is serviced from the 100 Hz timer tick
  (and a dedicated net task when work is pending); MSI/PLIC interrupts are a
  later optimization, consistent with the virtio-blk path.
- **Memory**: mbufs come from a pool backed by the frame allocator; no heap
  dependency (F3 lands later). NetBSD's `pool(9)` is imported for this.
- **Time**: NetBSD `callout(9)` is adapted to our timer tick.
- **Locking**: pre-SMP the rump-style locks are no-ops with assertions;
  they become real spinlocks when F2 (SMP) lands.
- **Deferred work**: `softint`-style callbacks run on a kernel task, not in
  interrupt context.

## 4. Import set (phased)

| Phase | Import | Notes |
|---|---|---|
| 1 | mbuf, pool, callout, if.c, ether, arp, ip4, icmp, udp, loopback | loopback + ping + UDP before any NIC |
| 2 | TCP (`tcp_input/output/timer/subr`) + socket layer (`uipc_socket`, `sosend/soreceive`) | blocking socket calls run on kernel tasks |
| 3 | DHCP client (`dhcpcd`, BSD-2), DNS resolver (NetBSD libc source), route/ifconfig basics | autoconf + name resolution |
| 4 | TLS: mbedTLS (Apache-2.0) platform port; HTTP/HTTPS client tool | `wget`-equivalent |

Non-goals for v0.0.3: IPv6 (imported later from the same source), IPsec,
ALTQ, FIBs/VNET, BPF (a later diagnostics option), Wi-Fi, and any GPL code.

## 5. Compatibility shims to write

| Shim | Host side |
|---|---|
| `mb_alloc`/`m_free` cluster pool | frame allocator, 2 KiB clusters |
| `pool(9)` | a small slab over contiguous frames |
| `callout(9)` | timer tick (100 Hz) with a sorted list |
| `mutex`/`rwlock` | pre-SMP: assertion no-ops; then spinlocks |
| `kthread`/softint | kernel tasks (`kernel_core::task`) |
| `sysctl` | read-only stub returning defaults |
| `microtime` | x86 TSC / arm64 generic timer via `kernel-core::time` |

Every shim is ours, <=300 lines, and unit-tested before the stack is
attached.

## 6. Driver interface (`net_ops`)

```c
struct net_ops {
    const char *name;                       /* "virtio-net" / "e1000" */
    int  (*init)(void *priv);
    void (*mac)(void *priv, uint8_t out[6]);
    int  (*send)(void *priv, const void *frame, size_t len);
    /* Returns >=0 length, 0 when nothing is pending, -1 on error. */
    int  (*recv)(void *priv, void *frame, size_t max);
    int  (*link)(void *priv);               /* 1 up, 0 down */
};
int net_register(const struct net_ops *ops, void *priv);
int net_send(const void *frame, size_t len);      /* first device */
int net_recv(void *frame, size_t max);
```

Drivers are C (like AHCI/NVMe): virtio-net reuses the virtio-mmio transport
code from M9 and gains a PCI transport for x86; e1000 is the QEMU x86
default. Both use `k_alloc_page` DMA memory and the existing `k_*` helpers.

## 7. Client API and tools

- Kernel-side socket wrapper (create/connect/send/recv/close) used by the
  built-in tools; the M14 syscall layer exposes the same calls to userspace
  (append-only numbers in `abi`).
- `ping` (ICMP), `nslookup` (resolver), `wget` (HTTP/HTTPS with progress).
- Diagnostics: link status, DHCP lease, route table, per-interface counters.

## 8. TLS policy

- mbedTLS with a platform config (`MBEDTLS_PLATFORM_*`, no filesystem, our
  entropy and time callbacks).
- Entropy: x86 RDRAND/RDSEED when present, arm64 `RNDR` when present, else
  `rdtime` mixed with device timing; virtio-rng when available.
- Certificates: a pinned CA bundle from the image; verification is on by
  default and a `--insecure` flag prints an explicit warning. The existing
  claim boundary holds: structure/self-consistency only, no chain-trust
  marketing claims.

## 9. Work breakdown (suggested commits)

| Step | Deliverable |
|---|---|
| M11-1 | Shims: mbuf/pool/callout/locks over frames and the tick, unit-tested |
| M11-2 | `net_ops` registry + loopback driver; ping over loopback |
| M11-3 | IPv4/ARP/ICMP/UDP on loopback; counters and diagnostics |
| M11-4 | TCP + socket layer; loopback throughput and loss tests |
| M11-5 | virtio-net (MMIO; PCI on x86) and e1000; DHCP client |
| M11-6 | DNS resolver + `ping`/`nslookup`/`wget` tools |
| M11-7 | mbedTLS port + HTTPS in `wget` + TLS KATs |
| M11-8 | arm64 bring-up of the same stack; smoke phases; docs/THIRD_PARTY update |

## 10. Spikes before coding

- Compile a minimal rump-style subset (mbuf + pool + loopback + ip/icmp) in
  a scratch directory and measure size/portability problems.
- Verify `callout` semantics against our 100 Hz tick (drift, chaining).
- mbedTLS on bare metal: platform config, entropy, footprint.
- QEMU fixtures: user-net packet path and a local HTTP/HTTPS test server.

## 11. Verification

- Loopback: ICMP, UDP and TCP tests (echo server), no packet loss under a
  bounded stress loop.
- QEMU user-net + a host fixture server: DHCP, DNS, HTTP GET, HTTPS GET,
  and a transfer larger than the socket buffers.
- Both x86_64 and arm64; riscv virtio-net is a follow-up if time allows.
- All existing smokes stay green; `smoke.sh`/`smoke-riscv.sh` gain network
  phases once the tools exist.

## 12. Risks

| Risk | Mitigation |
|---|---|
| The import balloons (NetBSD net is large) | phased subset above; measure at each phase; drop unused subsystems (IPv6/IPsec/ALTQ) explicitly |
| Upstream drift | record the exact commit; keep changes in `third_party/netbsd/` and shims outside it |
| Locking model clashes | pre-SMP shims are no-ops with assertions; F2 converts them to real locks |
| 300-line rule vs imported files | policy: imports are exempt and registered; our wrappers stay <=300 |
| TLS footprint/entropy | spike first; embedded-tls as a lighter fallback if mbedTLS is too heavy |
