# M11 plan (v0.0.3): ARM64 + NetBSD-derived network

Status: approved 2026-09. One commit per R batch with a "Verified:"
paragraph; after each batch we STOP so the owner can push before the next
one starts (small, archived deltas instead of one big drop).

## Owner decisions

- Strict order M11 -> M12 -> M13 (0.0.3/0.0.4/0.0.5); M12's remaining
  items are independent and may run as short side batches.
- Network stack: **full rump subset port** - vendor `sys/kern` +
  `net`/`netinet` and their dependencies under `third_party/netbsd/`, with
  an adaptation layer replacing `rumpuser` (memory -> frame allocator,
  threads -> cooperative tasks, callouts -> the PIT tick, locks ->
  spl/atomics, printf -> serial log). Imported files keep their headers,
  are exempt from the 300-line rule and are registered per file.
- ARM64: **both boot paths** - direct FDT boot on QEMU `virt` first
  (riscv-like, no firmware), then the UEFI loader path under AAVMF
  (verify `aarch64-unknown-uefi` availability during the R8 spike).
- Network verification: **offline gate always**, plus **optional external
  checks via the host's Tor SOCKS proxy** at `127.0.0.1:9050`. A host-side
  TCP->SOCKS5 relay (`tools/tor_relay.py`, stdlib only) lets the guest
  reach the internet through `10.0.2.2:<port>`; hostname resolution goes
  through the relay (socks5h semantics) so GFW DNS pollution cannot turn a
  reachable target into a failure. Without Tor the external phase prints
  `skip (no tor)`; external failures are recorded, never gate failures.
- M12-6 AMD GPU is accepted QEMU-only; the real-hardware report stays a
  documented follow-up.

## Definition of done (v0.0.3)

- Every M11 checkbox in `PROGRESS.md` closed (or deferred with a reason).
- `tools/smoke-net.sh` offline gate PASS on x86_64 and aarch64; existing
  smokes (`smoke.sh`, `smoke-bios.sh`, `smoke-riscv.sh`, `smoke-iso.sh`)
  stay green.
- aarch64 smoke phases for both boot paths (direct FDT + UEFI).
- `THIRD_PARTY.md` register complete for the rump import and mbedTLS;
  docs/version strings at 0.0.3; owner pushes and tags v0.0.3 locally.

## Batches

### R1 - rump vendor + adaptation spike (largest risk, first)

Scope: import the first slice (mbuf/pool, callout, mutex/rwlock/condvar,
spl, kernel subr used by them) into `third_party/netbsd/` with a curated
file list and a build recipe; write the adaptation layer (`kern_shim`):
alloc/free over the frame allocator, thread context -> cooperative
tasks, callouts driven by the PIT tick, panic/printf -> serial. Prove it by
running an allocation/refcount/callout self-test task in QEMU.

- Outcome (2026-09 spike, see `third_party/netbsd/ADAPTATION.md`): 139
  files pinned at `e145e524ee8362fa7d14824b2921b0ba1b694bfe`; all 9 imported
  `.c` files compile freestanding for `x86_64-unknown-none`
  (`third_party/netbsd/build_spike.sh` 9/9 OK) against 27 machine + 22
  config shim headers; the link leaves 104 unresolved NetBSD services. R2
  starts with libkern/atomics, the frame-backed kmem/pool, then the PIT
  callout tick and cooperative sleepq/turnstile; the QEMU refcount/callout
  self-test moves to R2 with those services.
- Verify: three arch builds zero warnings; `THIRD_PARTY.md` register plus
  `third_party/netbsd/MANIFEST.tsv` cover every imported file.
- Risk: source-list/build integration; missing NetBSD subr pulls; licenses.

### R2 outcome - the adapter took the slot

R2 was spent on the adaptation layer instead of net_ops (the R1 link
gaps came first): `kernel-net` compiles the nine imported objects plus
eleven adapter C files for x86_64 and links with 0 unresolved symbols
(104 before). The in-kernel self-test passes at boot: mbuf 12/12,
pool 8/8, callout 20 fires driven by IRQ0. Known stubs to replace as the
stack grows: sleepq/turnstile panic on a real block, kmem/vmem do not
recycle, xc_* is synchronous, percpu_* is single-CPU and sysctl is
read-only. The remaining batches shift one slot: R3 net_ops + loopback,
R4 IPv4/ARP/ICMP/UDP, R5 TCP/socket, R6 drivers/DHCP, R7 DNS/tools,
R8 mbedTLS/HTTPS, R9 aarch64 + release (tracker items M11-2..8 unchanged).

### R3 outcome - net_ops, loopback and ping in the kernel

R3 extended the import with the real ifnet/route slice (`if.c`, `if_loop.c`,
`route.c`, `radix.c`, `rtbl.c`, `if_stats.c`, `bpf_stub.c`,
`subr_pserialize.c`; 197 files total) and attached lo0 with 127.0.0.1/8 from
the `net_ops` registry (`net_register`/`net_ifattach`/`net_ifdetach`/
`net_send`/`net_recv`). The boot ping goes out through `if_output` ->
`looutput` -> the adapter packet queue and comes back as an ICMP echo reply
turned around in place; R4 replaces the adapter responder with
`ip_input.c`/`ip_icmp.c`/`in.c`. `tools/smoke-net.sh` gates the loopback
markers; its phase structure is ready for the R6 SLIRP and R8 optional Tor
phases.

### R4 - IPv4/ARP/ICMP/UDP

Scope: address config, ARP cache, ICMP echo, UDP sockets; diagnostics.
- Verify: loopback suite with counters; malformed-packet guards.

### R4 outcome - the real IPv4 stack on loopback

R4 imported the IPv4 closure (`ip_input.c`, `ip_output.c`, `ip_icmp.c`,
`ip_reass.c`, `in.c`, `in_pcb.c`, `in_proto.c`, `udp_usrreq.c`,
`if_arp.c`, `in_cksum.c`, `in4_cksum.c`, `cpu_in_cksum.c`,
`in_offload.c`, `if_llatbl.c`, `nd.c`, `subr_hash.c`, `subr_once.c` and
their headers; 246 files total) and deleted the R3 ICMP responder.  lo0
now runs `ip_output` -> `looutput` -> the adapter pktqueue -> real
`ip_input` -> `icmp_input`, and the boot ping reads its echo reply from
the R4 `rip_input` stub.  A UDP exchange over two real `inpcb`s bound to
127.0.0.1 checks payload/hash equality and PCB states, and an ARP
request/reply pair on a shim ethernet interface exercises `arpintr` ->
`in_arpinput` -> `arpresolve` and the real lltable cache.  The remaining
stubs are peripheral to this path: TCP, raw sockets, IGMP, encapsulation,
the socket layer, portalgo and pktqueue softint scheduling (single-CPU
list drained by the net task).  `tools/smoke-net.sh` asserts the R4
markers; `smoke-bios.sh` (2/2) and `smoke-riscv.sh` (3/3) stay green.

### R5 - TCP + socket layer

Scope: TCP state machine on the rump stack, socket API for the kernel
client, loss/throughput tests over loopback.
- Verify: transfer of a known blob with hash equality under an injected
  drop/reorder shim.

### R5 outcome - the real socket and TCP layer

R5 imported the real TCP and socket layers (`sys/netinet/tcp_input.c`,
`tcp_output.c`, `tcp_subr.c`, `tcp_timer.c`, `tcp_usrreq.c`,
`tcp_congctl.c`, `tcp_sack.c`, `tcp_syncache.c`,
`sys/kern/uipc_socket.c`, `uipc_socket2.c` and the `tcp_private.h`/
`tcp_congctl.h`/`tcp_syncache.h` headers; 259 files total) and deleted the
R4 socket/TCP stubs (`rump_sock2.c`, the `tcp_*` panic paths).  `soinit()`
now supplies the socket cache/`softnet_lock`, and `rump_tcp.c` drives a
real `socreate`/`sobind`/`solisten`/`soconnect`/`soaccept`/`sosend`/
`soreceive`/`soshutdown`/`soclose` client (non-blocking, polled from the
net task) over the real `tcp_input`/`tcp_output`.  The boot test transfers
a deterministic 64 KiB blob with byte/hash equality, closes gracefully,
then repeats with the first two data segments dropped by an adapter loss
hook in `pktq_enqueue` and reports the retransmit, plus a PIT-tick
throughput number.  Remaining stubs (raw sockets, IGMP, portalgo, vtw,
select/kqueue no-ops, blocking waits) are listed in `ADAPTATION.md`;
`tools/smoke-net.sh` asserts the R5 markers.

### R6 - virtio-net + e1000 + DHCP

Scope: virtio-net (MMIO first; PCI on x86_64), e1000, DHCP client.
- Verify: SLIRP lease acquired; ARP/ICMP to 10.0.2.2.

### R6 outcome - the e1000, DHCP and the SLIRP offline phase

No new NetBSD files were needed: the driver, the DHCP client and the HTTP
client are all adapter code.  `rump_e1000*.c` probes the QEMU 82540EM over
the kernel's PCI hooks (new `Env` callbacks `pci_read`/`pci_write`/
`mmio_map`/`virt_to_phys`), resets it, reads the MAC (RA[0], EEPROM
fallback), builds 32-entry RX/TX rings in contiguous frame pages and runs
interrupt-free from `rump_net_poll` (drained before `rump_pktq_drain`).
Receive wraps frames in cluster mbufs and demuxes the ethertype into
`ip_pktq`/`arp_pktq` (the `ether_input` counterpart, since
`if_ethersubr.c` is not imported); `if_csum_flags_{tx,rx}` stay 0 and the
driver finishes TCP/UDP checksums on the linearized TX buffer.  `rump_dhcp.c`
runs DISCOVER/OFFER/REQUEST/ACK over a real UDP socket (`0.0.0.0:68` ->
`10.0.2.2:67`, xid-checked, 5 tries, 1.2 s apart) after giving the NIC a
provisional link-local 169.254.1.1/16 and a host route to the server, then
applies the lease through `in_control(SIOCAIFADDR)`/`SIOCDIFADDR` and the
gateway through `rtrequest1`.  `rump_http.c` fetches the host fixture with
the real TCP socket layer.  `workqueue_enqueue()` became genuinely deferred
(softintd drains it): running route free work synchronously deadlocked
`rt_free_global.lock` during the provisional-address teardown.
`tools/run.sh --net` attaches the e1000 on SLIRP (default boots are
NIC-less, `-nic none`); `tools/smoke-net.sh` gained the SLIRP phase with a
python HTTP fixture on 127.0.0.1:18080 and byte/hash assertions.  virtio-net
is deferred to R9 with the MMIO transport (documented in `ADAPTATION.md`).

### R7 - DNS + tools

Scope: resolver, `ping`/`nslookup`/`wget` as shared shell commands.
- Verify: offline DNS server (host-side) + fetch from the local HTTP
  server; external phase is optional per the Tor policy.

### R7 outcome - the offline DNS fixture and the tools

The resolver (`rump_dns.c` + `rump_dns_pkt.c`) is a bounded UDP client on
the real socket layer: query id, question echo and RCODE are verified,
three sends 1 s apart, and the DHCP option-6 address is the default server
with an explicit override for the boot self-test and `nslookup`.  The
tools are x86_64 shell commands (`kernel/src/shell/cmds_net.rs`) behind
`CONFIG_TOOLS`; when `CONFIG_NET=n` they are one-line stubs, so minimal
builds contain no client code.  A shell command does not call into the
stack itself: `rump_toolreq.c` is a request slot the net task steps.  Two
latent bugs surfaced while adding the second (shell) task: the x86_64
context switch now saves/restores RFLAGS (a task could resume from a
timer-interrupt switch with IF=0 and freeze the PIT), and the R5 TCP test
timeout is tick-based instead of net-task iterations.  The R7 boot
sequence (`rump_tools.c`) resolves `test.fantuan` against the smoke's
authoritative UDP DNS fixture (10.0.2.2:5353), ICMP-echoes the resolved
address and wgets the R6 HTTP fixture by name.  `tools/smoke-net.sh`
gained `phase_dns_tools` with both host fixtures and a paced serial feeder
that exercises `nslookup`/`ping`/`wget`/`help` in the shell after the boot
self-test; the phase asserts the exact markers plus the shell transcripts.
### R8 - mbedTLS port + HTTPS + KAT

Scope: vendored mbedTLS config for the kernel, TLS client in `wget`, TLS
KATs (SHA-256/RSA/AES-GCM) in the boot diagnostics.
- Verify: offline TLS server with a pinned CA; KAT output line; external
  HTTPS through Tor when available.

### R9 - aarch64 bring-up + final docs

Scope: direct FDT boot on QEMU `virt`; UEFI loader path under AAVMF; the
full stack on aarch64; smoke phases; docs/matrices/THIRD_PARTY; 0.0.3.
- Verify: aarch64 direct + UEFI smokes; smoke-net offline gate; all other
  smokes green; version strings at 0.0.3.

## Checkpoint protocol

1. Implement the batch; build all arches with zero warnings; run the
   affected smokes (and the fast cross-arch ones).
2. Commit with a "Verified:" paragraph; update `PROGRESS.md`.
3. STOP and tell the owner "push now"; do not start R(n+1) until the owner
   confirms the push. This keeps every pushed state small and archived.

## Network test environment

- Offline gate: `-netdev user,id=n0` (SLIRP) with host-side HTTP/DNS/TLS
  servers bound to `127.0.0.1` and reachable from the guest as `10.0.2.2`;
  a pinned CA is generated per run under `build/`.
- External (optional), via `tools/tor_relay.py` listening on a host port
  and forwarding to `127.0.0.1:9050` with SOCKS5 (the guest targets
  `10.0.2.2:<port>`; enabled only when 9050 is reachable):
  1. `github.com` - loose proxy detection, first reachability rung;
  2. `x.com` (Twitter) - Tor is not blocked there;
  3. `duckduckgo.com` - Tor-friendly, serves the search page;
  4. **Duck.AI interactive round**: one dialogue turn against
     DuckDuckGo's AI chat (no login) proving the guest can POST user input,
     accept and store the session cookie, and read back a model response.
     Cookies persist in the guest for the run; no credentials are used.
- Conformance: plain HTTP(S) fetches assert status/body markers; the AI
  turn asserts a non-empty response body bound to the submitted prompt.
- Never gate on external results; each is recorded as PASS/SKIP/FAIL in the
  run report and a skipped Tor phase still leaves the offline gate green.
