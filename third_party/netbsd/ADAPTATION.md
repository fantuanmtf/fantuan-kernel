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

`tools/import_netbsd.sh` classifies each file's header: 76 BSD-2-Clause,
59 BSD-3-Clause, 2 Public-Domain, 1 MIT (the CMU/MIT-style `sys/cdefs_elf.h`).
One file, `sys/sys/device_if.h`, has no per-file notice at all (NetBSD
committed generated header); it is imported as-is and recorded as `UNKNOWN`
in `MANIFEST.tsv`. This is the only license item needing owner review for the
distribution; it is NetBSD project source and is covered by NetBSD's
BSD-licensed tree, but the header itself makes no statement.
