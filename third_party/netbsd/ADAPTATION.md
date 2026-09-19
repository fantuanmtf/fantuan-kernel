# NetBSD rump slice - adaptation layer (R1)

The imported source is pinned to `refs/heads/netbsd-10` of the GitHub mirror
`NetBSD/src` at commit `e145e524ee8362fa7d14824b2921b0ba1b694bfe`
(2026-09-18). `MANIFEST.tsv` enumerates every file, its sha256 and license;
`tools/import_netbsd.sh` is the only writer of that tree.

Everything in this file is derived from an actual build: `third_party/netbsd/
build_spike.sh` compiles the 9 imported `.c` files with clang 22.1.8 as
freestanding `x86_64-unknown-none` objects and then links them with a tiny
`_start` stub. Measured on the pinned tree:

- compile: **9 OK / 0 FAIL** (the per-file table is printed by the script);
- link: **104 unresolved NetBSD services** (the R2 work order).

The imported `.c` files are `subr_evcnt.c`, `kern_mutex.c`, `kern_condvar.c`,
`kern_rwlock.c`, `kern_lock.c`, `kern_timeout.c`, `subr_psref.c`,
`subr_pool.c` and `uipc_mbuf.c`.

## 1. Why an adaptation layer instead of rumpuser

NetBSD's rump kernels run the same sources on a host by linking them against
`rumpuser` (host threads, host mmap, host clocks, host console). That layer is
designed for a POSIX host and is not part of the imported slice. The fantuan
port keeps the NetBSD kernel sources unmodified and supplies the host side
itself, split in two:

- `shim/include/` (ours, kept under the repo's 300-line rule): the
  machine-dependent and config-generated headers the MI sources expect;
- a future `kern_shim` C layer (R2) implementing the services in section 2.

The R1 shim does not implement a working kernel - it makes the sources parse
and compile, and deliberately leaves every platform service undefined so the
linker enumerates them.

## 2. Service mapping (planned for R2; symbols are measured)

| NetBSD / rumpuser service | fantuan replacement | unresolved symbols measured in R1 |
|---|---|---|
| alloc/free, pooled memory | `kernel_core::frame` + bump/heap shim | `kern_malloc` `kern_free` `kmem_alloc` `kmem_free` `kmem_zalloc` `kmem_meta_arena` `kmem_va_arena` `uvm_km_kmem_alloc` `uvm_km_kmem_free` `vmem_alloc` `vmem_free` `nkmempages` `physmem` |
| threads / LWP context | cooperative `kernel_core::task` | `curlwp` `kpreempt` `lwp_pctr` `lwp_unlock_to` |
| mutexes, locks, barriers | spl masking + compiler atomics | `mutex_enter` `mutex_exit` `mutex_obj_alloc` `mutex_spin_enter` `mutex_spin_exit` `_atomic_cas_ulong` `atomic_cas_ptr` `atomic_swap_ptr` `atomic_inc_uint` `atomic_inc_uint_nv` `atomic_dec_uint_nv` `membar_producer` `membar_release` `lockdebug_abort` `pserialize_not_in_read_section` |
| interrupt priority | x86 CR8/IF masking, later arch-specific | `splraise` `spllower` |
| sleep/wakeup queues | park/unpark on cooperative tasks | `sleepq_init` `sleepq_enqueue` `sleepq_block` `sleepq_wake` `sleepq_remove` `sleepq_unsleep` `sleepq_changepri` `sleepq_lendpri` `sleepq_locks` `sleep_syncobj` `syncobj_noowner` `turnstile_lookup` `turnstile_block` `turnstile_wakeup` `turnstile_unsleep` `turnstile_changepri` `turnstile_exit` |
| callout(9) | PIT tick on x86_64/i686, riscv timer | `getticks` `hz` `time_uptime` |
| softint | deferred callbacks on a kernel task | `softint_establish` `softint_schedule` `cpu_intr_p` `cpu_softintr_p` |
| per-CPU data / cross-calls | single-CPU arrays and no-op calls until F2 (SMP) | `percpu_alloc` `percpu_free` `percpu_getref` `percpu_putref` `percpu_foreach` `xc_barrier` `xc_broadcast` `xc_encode_ipl` `xc_wait` `cpu_info_primary` `mp_online` `ncpu` |
| printf / panic | kernel log (serial) | `printf` `snprintf` `panic` `panicstr` `log` `ratecheck` `cold` `nullop` |
| sysctl | read-only stub returning defaults | `sysctl_createv` `sysctl_lookup` `sysctl_query` `sysctl_copyout` `sysctl_unlock` `sysctl_relock` `copyout` |
| libkern helpers | kernel runtime (or libkern subset imported later) | `memset` `memcpy` `memmove` `strlen` `strcmp` `strcpy` `strlcpy` `hash_value` |
| interface registry / mbuf tuning | `net_ops` registry (R2); tunables become constants | `domains` `if_acquire` `if_put` `ifnet_pslist` `msize` `mclbytes` `mblowat` `mcllowat` `nmbclusters` |
| misc | stubs / real values when needed | `coherency_unit` `get_expose_address` |

`get_expose_address` is only referenced by the KASAN/MSAN instrumentation
paths; `coherency_unit` is a cache-line constant on x86_64.

## 3. What the shim covers today (compile-only)

`shim/include/machine/` contains 27 hand-written x86_64 headers replacing the
NetBSD arch tree: `types.h`, `param.h`, `limits.h`, `int_*.h`, `endian*.h`,
`cdefs.h`, `ansi.h`, `wchar_limits.h`, `bswap.h`, `signal.h`, `mcontext.h`,
`pmap.h`, `vmparam.h`, `intrdefs.h`, `intr.h` (IPL constants,
`makeiplcookie`/`splraiseipl`, `sys/spl.h` glue), `lock.h` (compiler-atomic
simple locks), `mutex.h` (`struct kmutex` layout), `rwlock.h`, `proc.h`,
`pcb.h`, `cpu.h`, `cpu_counter.h`. `shim/include/opt_*.h` are 22 empty
config headers emulating a `config(1)` build with no options set (`_KERNEL_OPT`
is intentionally not defined).

The real NetBSD x86 assembly/lock/APIC layer is **not** copied; that is why
`splraise`/`spllower` and the scheduler services above are unresolved. The
shim targets x86_64 only; i686 and riscv machine shims are R2/R8 work, and the
shim directory will be split per architecture when the second target lands.

## 4. Recommended dependency order for R2

1. **libkern subset + atomic primitives** - self-contained, unblocks every
   other object (`memset`, `memcpy`, `strlen`, `atomic_*`, `membar_*`).
2. **frame-backed `kern_malloc`/`kmem_*`/`vmem_*`** - the pool allocator calls
   `vmem_alloc`/`vmem_free` directly; implement over `kernel_core::frame` and
   size the arenas empirically.
3. **spl + mutex spin stubs** - compile a boot self-test that calls
   `pool_init`/`pool_get`/`pool_put` and `callout_init`/`callout_schedule`.
4. **callout tick** - `getticks`, `hz`, `time_uptime` from the PIT/timer;
   verify periodic firing with the R1 self-test.
5. **sleepq/turnstile over cooperative tasks** - needed by condvar/rwlock
   wait paths and by `kern_timeout`'s blocking calls.
6. **softint + per-CPU + xcall stubs** - single-CPU semantics first.
7. **sysctl/printf/panic/log** - diagnostics so the self-test output is
   readable.
8. **net_ops + if.c + loopback** - the actual R2 goal; then the mbuf tunables
   become constants and `domains`/`ifnet_pslist` get real storage.
   `subr_pool.c` can run either on the `vmem` shim or on `kmem_*` alone if the
   vmem dependency is cut later.

## 5. License notes

`tools/import_netbsd.sh` classifies each file's header; for the 259-file R5
tree: 124 BSD-2-Clause, 126 BSD-3-Clause, 2 Public-Domain, 1 MIT (the
CMU/MIT-style `sys/cdefs_elf.h`), 1 Beerware (`sys/sys/timetc.h`, the
Poul-Henning Kamp licence text) and 5 with no per-file notice at all.  The
five are committed NetBSD-generated or historical headers: `device_if.h`,
`in_selsrc.h`, `cprng_fast.h`, `in_ifattach.h` (thin prototype header) and
`ip_mroute.h` (RCS id plus a historical BBN note).  They are imported as-is
and recorded as `UNKNOWN` in `MANIFEST.tsv`; all are NetBSD project source
covered by NetBSD's BSD-licensed tree, but the headers themselves make no
statement, so they remain the license items needing owner review for the
distribution.

## 6. R2 outcome (2026-09): the adapter runs in-kernel

The adapter lives in the `kernel-net` no_std crate (`kernel-net/src/c/*.c`),
whose `build.rs` compiles the nine imported `.c` files and eleven adapter files
with clang for `x86_64-unknown-none` against the same shim include path as
`build_spike.sh`. The crate has no kernel-core dependency: the x86_64 kernel
installs an `Env` of callbacks (kernel log, frame pages, PIT ticks, task
sleep/exit) and spawns `kernel_net::softintd` and `kernel_net::selftest_task`.
riscv64 and i686 do not link the crate. C flags add `-mcmodel=large` and the
soft-float/`-mno-sse` set so the BIOS path (no CR4.OSFXSR) cannot take #UD.
`shim/include/machine/mutex.h` no longer claims stub mutexes: the MI
`kern_mutex.c` uniprocessor paths (CAS adaptive mutex, SPL-only spin mutex)
are now the implementation; contended paths assert.

Implemented services (R1 dependency order): libkern strings/atomics; a bump
kmem/kern_malloc plus frame-backed `uvm_km_kmem_alloc/free` (real page free)
and vmem stubs; MI mutex/rwlock/condvar over SPL masks; the LWP/CPU sentinel
(`curlwp`, `cpu_info_primary`, `lwp0`); sleepq/turnstile asserted; callouts
driven by `callout_hardclock()` from the PIT; softints as the `softintd` task;
percpu single-CPU; xcall synchronous/no-op; printf/snprintf/panic/log/
ratecheck to the kernel log; read-only sysctl stubs and bounded `copyout`;
empty network registry and mbuf tunables. `link.ld` defines the NetBSD
`link_set_evcnts` bounds inside `.rodata` so `evcnt_init()` iterates.

Measured link of the whole slice (all 9 imported objects + 10 adapter objects,
forcing every archive member, with a tiny hook stub): **0 unresolved
symbols**, down from the R1 count of 104. No unused-archive exclusion is
needed.

Boot self-test (x86_64 UEFI and BIOS, `rump-selftest` feature, on by
default):

```
rump: mbuf self-test ok (allocs=12 frees=12)
rump: pool self-test ok
rump: callout self-test ok (fires=20)
```

The test task is bounded (200 ticks ≈ 2 s for the callout phase), exits, and
stays quiet. Built with `--no-default-features` the adapter still comes up
and the softint drainer runs, but no self-test task exists and no `rump:`
lines are printed.

Stubs that assert/panic if a real scheduler wait is required in R2:
`sleepq_block/enqueue/wake`, `turnstile_block`; `pserialize` is a no-op,
`xc_*` runs callbacks synchronously, `percpu_*` is one CPU. `kmem_free`,
`vmem_free` and `kern_free` do not recycle; R3 replaces the bump arena with a
real vmem/kmem before the socket layer allocates in loops.

## 7. R3 outcome (2026-09): net_ops, loopback and ping

The import grew to 197 files (106 BSD-2, 85 BSD-3, 2 Public-Domain, 1
MIT/CMU, 3 without a per-file notice: `device_if.h`, `in_selsrc.h`,
`cprng_fast.h`; all NetBSD project source under the tree's BSD terms). The
R3 `.c` additions are the real `sys/net/if.c`, `sys/net/if_loop.c`,
`sys/net/route.c`, `sys/net/radix.c`, `sys/net/rtbl.c`,
`sys/net/if_stats.c`, `sys/net/bpf_stub.c` and
`sys/kern/subr_pserialize.c` plus their header closure (if_dl/if_ether/
if_media/if_types/kauth/module/net80211/compat/...). Two shim-generated
config headers join `shim/include/`: `ether.h`, `bridge.h`, `carp.h` (all
counts 0). `kernel-net/build.rs` defines `__NetBSD__` (NetBSD-specific
header paths) and `INET` (the IPv4 paths) for the imported objects.

`kernel-net/src/c/rump_shim_net.c` now owns the `net_ops` registry from
`docs/M11_NET.md` section 6 (`net_register`/`net_ifattach`/`net_ifdetach`/
`net_send`/`net_recv`), a single-CPU mbuf packet queue standing in for
`pktqueue(9)`, and the domain-list helpers. `rump_loopback.c` brings up the
real `if.c` core + `if_loop.c` lo0 (`rt_init`, `bpf_setops`, `ifinit1`,
`ifinit`, `loopattach`), assigns 127.0.0.1/8 via `ifa_insert`, and
registers the loopback as the first `net_ops` device; `rump_ping.c` runs
the boot ping. The output path is `if_output` -> `looutput` -> the shim
queue; the loopback task drains it, turns an ICMP echo request around in
place (addresses swapped, type 8 -> 0, IP/ICMP checksums redone) and feeds
the reply back through `if_output`; the ping client reads it via the
driver's `recv` op and times it with the PIT tick. R4 replaces this echo
responder with `ip_input.c`/`ip_icmp.c`/`in.c`; routing-socket
notifications (`rt_*msg`), pfil, hooks, kauth/module calls, the
single-CPU workqueue (synchronous), `pktqueue` and `sockaddr_*` helpers
are the remaining adapter stubs. The self-test, softint and sysctl stubs
from R2 are unchanged.

Boot markers (UEFI and the `tools/smoke-net.sh` loopback gate):

```
net: lo0 up 127.0.0.1/8
net: ping 127.0.0.1 ok (seq=1 rtt=10 ticks)
net: icmp echo reply ok
net: in/out counters pkts_in=2 pkts_out=2
```

## 8. R4 outcome (2026-09): the real IPv4 stack

The import grew to 246 files. The R4 `.c` additions are the IPv4 core and
its checksum/link-layer support: `sys/netinet/{ip_input,ip_output,ip_icmp,
ip_reass,in,in_pcb,in_proto,udp_usrreq,if_arp,in_cksum,in4_cksum,
cpu_in_cksum,in_offload}.c`, `sys/net/{if_llatbl,nd}.c` and
`sys/kern/{subr_hash,subr_once}.c`, plus their header closure
(`in_pcb.h`, `ip_icmp.h`, `ip_private.h`, `icmp_var.h`/`icmp_private.h`,
`udp.h`/`udp_var.h`/`udp_private.h`, `in_ifattach.h`, `in_gif.h`,
`ip_mroute.h`, `igmp_var.h`, `wqinput.h`, `portalgo.h`, the unconditional
TCP/IPv6 header pulls of `tcp_vtw.h`/`in_proto.c`, `sys/once.h`,
`sys/timetc.h`, `if_gre.h`, `sys/pcq.h`).  (`ip_id.c` does not exist at
this commit: IP ID allocation is the `ip_newid`/`ip_randomid` inline pair
in `in_var.h`.)  `shim/include/` adds the
config-generated `arp.h` (with `NARP 1`, keeping in.c's ARP lltable
attachment), `arcnet.h`, `gif.h`, `gre.h`, `pfsync.h` and the `opt_*`
headers the IPv4 files include.

Bring-up (`kernel-net/src/c/rump_ip4.c`): attach `inetdomain`/`arpdomain`
to the adapter domain list, run every protocol `pr_init` from their
protosw arrays (so `ip_init`, `icmp_init`, `udp_init` and `arp_init`
create the packet queue, counters, PCB table and lltable), call
`lltableinit()` (main() does this upstream; it creates the llentry pool),
then `rt_init`/`ifinit`/`loopattach` and `in_control(SIOCAIFADDR)` for
lo0 127.0.0.1/8 and the shim ether 10.0.0.1/24.  `pktqueue.c` is replaced
by a single-CPU implementation in `rump_shim_net.c` (one mbuf list per
queue, callback drained by the net task); `softnet_lock` is a real MI
mutex.  `ip_output -> looutput -> pktq_enqueue(ip_pktq) -> ipintr ->
ip_input/ip_icmp` is the live packet path; the echo reply is delivered to
the boot ping by the R4 `rip_input` stub in `rump_shim_inet.c` (raw
sockets are R5).

Tests: the ping client (`rump_ping.c`) builds the ICMP echo and calls
`ip_output`, measuring the reply RTT from the PIT tick.  The UDP test
(`rump_udp.c`) creates two fake sockets with real `inpcb`s (bind
127.0.0.1:24001/24002, connect), sends through `udp_output` and reads the
delivered datagram from the receiver's `so_rcv` after `udp_input`; the
receive-path slice `sbappendaddr()` lives in `rump_sock2.c` until R5
imports `uipc_socket2.c`.  The ARP test (`rump_arp.c`) builds a shim
`IFT_ETHER` ifnet whose `if_output` captures frames, enqueues a synthetic
ARP request on the real `arp_pktq` (the ether_input equivalent) and
checks that `in_arpinput` learned the sender and reflected a reply, then
injects the peer's reply and resolves the entry through `arpresolve`.

Remaining stubs (documented in the adapter, R5 work): the socket layer
and raw sockets (`rump_sock2.c`, `rip_*`), TCP (`tcp_*` panic), IGMP,
encapsulation and portalgo (`rump_shim_inet.c`), synchronous wqinput and
task-context `kmem_intr_*` (`rump_shim_misc.c`), pktqueue softint
scheduling/sysctl, pfil hooks, route-socket notifications and kauth
(no listeners -> ALLOW), and the single-CPU `rw_enter`/`rw_exit` no-ops.

Boot markers (UEFI and `tools/smoke-net.sh`):

```
net: lo0 up 127.0.0.1/8
net: ip4 input ok (pkts_in=2)
net: ping 127.0.0.1 ok (seq=1 rtt=10 ticks)
net: icmp echo reply ok
net: udp loopback ok (sent=1 recv=1 bytes=32)
net: arp self-test ok (entries=1)
net: in/out counters pkts_in=3 pkts_out=3
```

## 9. R5 outcome (2026-09): the real socket and TCP layer

The import grew to 259 files.  The R5 `.c` additions are the real TCP state
machine and socket layer: `sys/netinet/{tcp_input,tcp_output,tcp_subr,
tcp_timer,tcp_usrreq,tcp_congctl,tcp_sack,tcp_syncache}.c` and
`sys/kern/{uipc_socket,uipc_socket2}.c`, plus the `tcp_private.h`,
`tcp_congctl.h` and `tcp_syncache.h` headers.  `tcp_vtw.c` is **not**
imported: the vestigial TIME_WAIT feature is off by default and its
`vtw_*` entry points are adapter stubs that record "not added" or assert
(`sys/sys/md5.h` is not imported either - it carries the RSA notice - so
`rump_md5.c`, ours, implements the RFC 1948 ISS hash the TCP code calls).

`rump_sock2.c` is gone: `sbappendaddr`/`sbcreatecontrol`/`sowakeup`/
`soisconnected`/`soreserve`/... are the imported implementations now.  Bring-up calls
`soinit()` (socket cache, `softnet_lock`, `sb_max`) before the protocol
`pr_init`s; the adapter's kernel socket client in `rump_tcp.c` uses the
real `socreate`/`sobind`/`solisten`/`soconnect`/`soaccept`/`sosend`/
`soreceive`/`soshutdown`/`soclose` path (non-blocking, polled from the net
task) with the real `tcp_input`/`tcp_output` over the loopback pktqueue.
New shim headers make the closure compile without importing the excluded
subsystems: `sys/{file,filedesc,poll,kthread,buf,md5}.h`,
`uvm/uvm_{loan,page}.h`, `netipsec/*`, `netinet6/{nd6,scope6_var,
in6_offload,ip6protosw}.h`, `netinet/sctp_route.h`, `ddb/db_active.h`,
`net/if_faith.h`, `compat/sys/socket.h` and the
`opt_tcp_*`/`opt_sb_max`/`opt_sosend_loan`/... config stubs.

The adapter side is split to keep every file under the 300-line rule:
`rump_shim_ksock.c` (select/kqueue no-ops, credential/uidinfo/`chgsbsize`
sentinels, the UVM-loan refusal so `sosend` always copies, `uiomove`,
`proc0`/`plimit`), `rump_md5.c` (RFC 1948 ISS hash), `rump_shim_mobj.c`
(reference-counted mutex objects - every socket shares `softnet_lock`),
`rump_domain.c` (domain list and lookups), `rump_loss.c` (deterministic
drop hook) and `rump_tcp.c` + `rump_tcp_conn.c` + `rump_tcp_io.c` (the
socket client and its loopback test).

The boot TCP test creates two real sockets on 127.0.0.1, completes the
3-way handshake, transfers a deterministic 64 KiB blob (FNV-1a hash
checked byte-for-byte), shuts down and closes gracefully, then repeats on
a second connection while the loopback output queue drops the first two
data segments (adapter loss injection in `pktq_enqueue`, keyed on
client->server TCP sequence ranges) and reports the retransmission that
recovers them.  Throughput is measured from PIT ticks.  `tools/smoke-net.sh`
asserts the new markers.

Remaining stubs after R5: raw sockets (`rip_*`), IGMP, encapsulation and
portalgo (`rump_shim_inet.c`), vestigial TIME_WAIT (`vtw_*`), synchronous
wqinput and `kmem_intr_*` (`rump_shim_misc.c`), pktqueue softint
scheduling/sysctl, pfil hooks, route-socket notifications, kauth (no
listeners -> ALLOW), single-CPU `rw_enter`/`rw_exit` no-ops and the
select/kqueue no-ops; blocking socket waits still assert (the client
polls).  R6 (drivers/DHCP) addresses the pktqueue/softint and driver side.

Boot markers added in R5 (`tools/smoke-net.sh`):

```
net: tcp connect ok (state=ESTABLISHED)
net: tcp transfer ok (bytes=65536 hash=ff8ebd03)
net: tcp throughput ok (bytes=65536 ticks=40)
net: tcp close ok (state=CLOSED)
net: tcp retransmit ok (drops=2 retrans=2)
net: in/out counters pkts_in=35 pkts_out=35
```

## 10. R6 outcome (2026-09): e1000, DHCP and the SLIRP offline phase

No NetBSD files were imported for R6 (the tree stays at 259 files): the
driver, the DHCP and the HTTP clients are adapter code.  `kernel-net` now
links `rump_e1000{,_dma,_if,_ops}.c`, `rump_dhcp{,_if,_pkt}.c` and
`rump_http.c` alongside the R5 split.

- `rump_e1000.c` probes the QEMU 82540EM (and the common 8254x/82574 IDs)
  over the kernel's PCI config hooks, maps BAR0 with the new
  `fantuan_rump_mmio_map()` (cache-disabled PhysOffset window; the hook
  returns the BAR's own virtual address, not the 2 MiB page base), resets
  it, reads the MAC from RA[0] (EEPROM fallback) and builds 32-entry
  descriptor rings plus packet buffers in contiguous frame-allocator pages.
- `rump_e1000_if.c`/`_ops.c` provide the ifnet: a minimal `ether_output`
  counterpart (`arpresolve`, AF_ARP target address, `M_PREPEND`,
  `ifq_enqueue` -> `if_start`), the net_ops table and the RX demux that
  feeds `ip_pktq`/`arp_pktq` (in place of the unimported if_ethersubr
  `ether_input`).  RX/TX are polled: `rump_net_poll()` drains the ring
  before `rump_pktq_drain()`.  `if_csum_flags_{tx,rx}` are 0 and
  `rump_e1000_dma.c` finishes TCP/UDP checksums in software on the
  linearized TX frame (the stack's offload partial is overwritten).
- `rump_dhcp.c` is a bounded client over the real UDP socket layer.  The
  unnumbered-DISCOVER problem is solved with a provisional link-local
  169.254.1.1/16 (`in_control(SIOCAIFADDR)`) and a host route to the server
  (`rtrequest1`); DISCOVER/OFFER/REQUEST/ACK run on a connected socket
  bound to `0.0.0.0:68` (its local address is then cleared so `udp_input`'s
  broadcast delivery matches), and the lease is applied through
  `in_control` plus a default route through `rtrequest1`.  The DHCP DNS
  option is kept for the R7 resolver.
- `rump_http.c` GETs the host fixture through the real TCP socket layer and
  hashes the entity body with FNV-1a.
- The `workqueue(9)` shim is now genuinely deferred: `workqueue_enqueue()`
  schedules and `softintd` drains via `rump_workqueue_drain()`.  The
  synchronous version re-entered `rt_free_global.lock` when the provisional
  address was deleted (route free work), which R6's teardown exposed.

virtio-net is **deferred to R9**: the MMIO transport belongs with the
riscv64/aarch64 work, and x86_64 QEMU has no virtio-net-mmio device; the
e1000 covers the offline gate.  This is a documented deferral, not a fake.

`tools/run.sh --net` attaches `-netdev user,id=n0 -device e1000,netdev=n0`;
all other x86_64 boots pass `-nic none` so the loopback phases stay NIC-less.
`tools/smoke-net.sh` keeps the LOOPBACK phase and adds the SLIRP phase: a
python stdlib HTTP server on 127.0.0.1:18080 with a deterministic 1408-byte
body, whose byte count and FNV-1a hash are asserted against the guest marker.

Boot markers added in R6 (SLIRP phase of `tools/smoke-net.sh`):

```
net: e1000 up mac=52:54:00:12:34:56
net: dhcp lease 10.0.2.15/24 gw 10.0.2.2 dns 10.0.2.3
net: http get ok (url=http://10.0.2.2:18080/ bytes=1408 hash=6bb06745)
net: eth counters pkts_in=9 pkts_out=11
```

Failure markers: `net: nic FAILED (<step>)`, `net: dhcp FAILED (<step>)`,
`net: http FAILED (<step>)`; a missing NIC is not a failure (the loopback
phases run silently without one).  Residual stubs unchanged from R5
(raw sockets, IGMP, encapsulation, portalgo, vestigial TIME_WAIT,
synchronous wqinput, select/kqueue no-ops, blocking waits); the IPv6/NDP
frames SLIRP emits are counted and dropped by the e1000 demux.
