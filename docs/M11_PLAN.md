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

### R2 - net_ops registry + loopback

Scope: `net_ops` driver registry, loopback interface, `ping` over it.
- Verify: loopback echo lines + counters in the smoke log.

### R3 - IPv4/ARP/ICMP/UDP

Scope: address config, ARP cache, ICMP echo, UDP sockets; diagnostics.
- Verify: loopback suite with counters; malformed-packet guards.

### R4 - TCP + socket layer

Scope: TCP state machine on the rump stack, socket API for the kernel
client, loss/throughput tests over loopback.
- Verify: transfer of a known blob with hash equality under an injected
  drop/reorder shim.

### R5 - virtio-net + e1000 + DHCP

Scope: virtio-net (MMIO first; PCI on x86_64), e1000, DHCP client.
- Verify: SLIRP lease acquired; ARP/ICMP to 10.0.2.2.

### R6 - DNS + tools

Scope: resolver, `ping`/`nslookup`/`wget` as shared shell commands.
- Verify: offline DNS server (host-side) + fetch from the local HTTP
  server; external phase is optional per the Tor policy.

### R7 - mbedTLS port + HTTPS + KAT

Scope: vendored mbedTLS config for the kernel, TLS client in `wget`, TLS
KATs (SHA-256/RSA/AES-GCM) in the boot diagnostics.
- Verify: offline TLS server with a pinned CA; KAT output line; external
  HTTPS through Tor when available.

### R8 - aarch64 bring-up + final docs

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
