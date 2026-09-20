#!/usr/bin/env bash
# M11 network smoke gate (offline).
#
# Phase LOOPBACK (R3/R4/R5): boot the x86_64 UEFI image with no NIC and
# assert the in-kernel markers of the real IPv4 path: ip_input/ip_output/
# ICMP echo over lo0, the UDP PCB exchange, the ARP self-test on the shim
# ether interface, the real socket/TCP connection (handshake, 64 KiB
# transfer with hash equality, graceful close) and the deterministic-drop
# retransmit run, plus the in/out counters.
#
# Phase SLIRP (R6): boot with `-device e1000 -netdev user,id=n0` and a
# host-side HTTP server on 127.0.0.1:18080, then assert the e1000 MAC/lease
# markers, the exact-body HTTP GET through `10.0.2.2:18080` and the eth
# counters.  R7 (DNS/tools) and R8 (TLS/Tor) append more phases below; the
# optional Tor phase never gates the offline ones.
#
# Output: "SMOKE PASS (net loopback)"/"(net slirp)" and the aggregate
# "SMOKE PASS (net offline gate)"; any failure exits non-zero.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build
export PATH="$HOME/.cargo/bin:$PATH"

# C4: the net gate is explicit — the default .config is the minimal shell.
python3 tools/kconfig.py --profile net >/dev/null \
  || { echo "SMOKE FAIL (net) - profile"; exit 1; }

NET_TIMEOUT="${NET_TIMEOUT:-60}"
HTTP_PORT="${HTTP_PORT:-18080}"
HTTP_LOG="build/smoke-net-http-server.log"
SRV_PID=""

cleanup() {
  if [ -n "$SRV_PID" ]; then
    kill "$SRV_PID" 2>/dev/null || true
    wait "$SRV_PID" 2>/dev/null || true
    SRV_PID=""
  fi
}
trap cleanup EXIT

phase_loopback() {
  local log="build/smoke-net-loopback.log"

  # Build outside the boot window: the config flip rebuilds the three kconfig
  # consumers and must not eat the QEMU timeout (run.sh then only relinks).
  rm -f "$log"
  ./tools/build.sh >/dev/null 2>&1 || { echo "SMOKE FAIL (net loopback) - build"; exit 1; }
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

# Host-side minimal HTTP fixture: deterministic body, FNV-1a hash printed
# on the READY line so the gate can compare the guest's marker exactly.
start_http_server() {
  local body_bytes body_hash

  rm -f "$HTTP_LOG"
  python3 - "$HTTP_PORT" > "$HTTP_LOG" 2>&1 <<'PY' &
import http.server, socketserver, sys
PORT = int(sys.argv[1])
BODY = b"fantuan-r6-slirp-body\n" * 64
class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.0"
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "application/octet-stream")
        self.send_header("Content-Length", str(len(BODY)))
        self.end_headers()
        self.wfile.write(BODY)
    def log_message(self, fmt, *args):
        pass
class S(socketserver.TCPServer):
    allow_reuse_address = True
h = 2166136261
for b in BODY:
    h = ((h ^ b) * 16777619) & 0xffffffff
with S(("127.0.0.1", PORT), H) as srv:
    print("READY %d %08x" % (len(BODY), h), flush=True)
    srv.serve_forever()
PY
  SRV_PID=$!
  for _ in $(seq 1 50); do
    grep -q "^READY " "$HTTP_LOG" 2>/dev/null && break
    sleep 0.1
  done
  if ! grep -q "^READY " "$HTTP_LOG" 2>/dev/null; then
    echo "SMOKE FAIL (net slirp) - HTTP server did not start:"
    cat "$HTTP_LOG"
    return 1
  fi
  body_bytes=$(awk '/^READY /{print $2}' "$HTTP_LOG")
  body_hash=$(awk '/^READY /{print $3}' "$HTTP_LOG")
  printf -v HTTP_BYTES '%s' "$body_bytes"
  printf -v HTTP_HASH '%s' "$body_hash"
}

phase_slirp() {
  local log="build/smoke-net-slirp.log"

  start_http_server || return 1
  rm -f "$log"
  ./tools/build.sh >/dev/null 2>&1 || { echo "SMOKE FAIL (net slirp) - build"; exit 1; }
  timeout --signal=KILL "$NET_TIMEOUT" ./tools/run.sh --net \
    > "$log" < /dev/null 2>&1 || true
  cleanup

  local lease="net: dhcp lease 10.0.2.15/24 gw 10.0.2.2 dns 10.0.2.3"
  local get="net: http get ok (url=http://10.0.2.2:$HTTP_PORT/ bytes=$HTTP_BYTES hash=$HTTP_HASH)"
  if grep -qE "net: e1000 up mac=([0-9a-f]{2}:){5}[0-9a-f]{2}" "$log" \
     && grep -qF "$lease" "$log" \
     && grep -qF "$get" "$log" \
     && grep -qE "net: eth counters pkts_in=[0-9]+ pkts_out=[0-9]+" "$log" \
     && grep -q "net: tcp connect ok (state=ESTABLISHED)" "$log" \
     && grep -q "net: tcp close ok (state=CLOSED)" "$log" \
     && ! grep -qE "net: (nic|dhcp|http) FAILED" "$log"; then
    echo "SMOKE PASS (net slirp)"
    grep -aE "net: (e1000 up|dhcp lease|http get ok|eth counters)" "$log"
    return 0
  fi
  echo "SMOKE FAIL (net slirp) - log tail:"
  tail -30 "$log"
  return 1
}

# R7: phase_dns_tools() { ... offline DNS + ping/nslookup/wget ... }
# R8: phase_tor()       { ... optional, never gates: skip (no tor) ... }

if ! phase_loopback; then
  exit 1
fi
if ! phase_slirp; then
  exit 1
fi
echo "SMOKE PASS (net offline gate)"
exit 0
