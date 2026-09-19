#!/usr/bin/env bash
# M11 network smoke gate (offline).
#
# Phase LOOPBACK (R3/R4/R5): boot the x86_64 UEFI image and assert the
# in-kernel markers of the real IPv4 path: ip_input/ip_output/ICMP echo over
# lo0, the UDP PCB exchange, the ARP self-test on the shim ether interface,
# the real socket/TCP connection (handshake, 64 KiB transfer with hash
# equality, graceful close) and the deterministic-drop retransmit run, plus
# the in/out counters.  R6/R8 append the SLIRP offline-server (HTTP/DNS) and
# the optional Tor phases below; each phase is a self-contained function so
# the earlier ones are not rewritten.  External results never gate the
# offline phases.
#
# Output: "SMOKE PASS (net loopback)" or "SMOKE FAIL (net loopback)"; any
# failure exits non-zero.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build
export PATH="$HOME/.cargo/bin:$PATH"

NET_TIMEOUT="${NET_TIMEOUT:-60}"

phase_loopback() {
  local log="build/smoke-net-loopback.log"

  rm -f "$log"
  timeout --signal=KILL "$NET_TIMEOUT" ./tools/run.sh \
    > "$log" < /dev/null 2>&1 || true

  if grep -q "net: lo0 up 127.0.0.1/8" "$log" \
     && grep -qE "net: ip4 input ok \(pkts_in=[0-9]+\)" "$log" \
     && grep -qE "net: ping 127.0.0.1 ok \(seq=[0-9]+ rtt=[0-9]+ ticks\)" "$log" \
     && grep -q "net: icmp echo reply ok" "$log" \
     && grep -qE "net: udp loopback ok \(sent=1 recv=1 bytes=[0-9]+\)" "$log" \
     && grep -qE "net: arp self-test ok \(entries=[0-9]+\)" "$log" \
     && grep -q "net: tcp connect ok (state=ESTABLISHED)" "$log" \
     && grep -qE "net: tcp transfer ok \(bytes=[0-9]+ hash=[0-9a-f]{8}\)" "$log" \
     && grep -qE "net: tcp throughput ok \(bytes=[0-9]+ ticks=[0-9]+\)" "$log" \
     && grep -q "net: tcp close ok (state=CLOSED)" "$log" \
     && grep -qE "net: tcp retransmit ok \(drops=[0-9]+ retrans=[0-9]+\)" "$log" \
     && grep -qE "net: in/out counters pkts_in=[0-9]+ pkts_out=[0-9]+" "$log" \
     && ! grep -q "net: ip4 FAILED" "$log" \
     && ! grep -q "net: tcp FAILED" "$log"; then
    echo "SMOKE PASS (net loopback)"
    grep -aE "net: (lo0 up|ip4 input ok|ping |icmp echo reply|udp loopback|arp self-test|tcp |in/out counters)" "$log" \
      | head -14
    return 0
  fi
  echo "SMOKE FAIL (net loopback) - log tail:"
  tail -30 "$log"
  return 1
}

# R6: phase_slirp() { ... host HTTP/DNS fixture over -netdev user ... }
# R8: phase_tor()   { ... optional, never gates: skip (no tor) ... }

if ! phase_loopback; then
  exit 1
fi
exit 0
