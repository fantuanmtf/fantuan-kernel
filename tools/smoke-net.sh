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
# Phase SLIRP (R6): boot with `-device e1000 -netdev user,id=n0` and the
# host fixtures from tools/net_fixtures.py, then assert the e1000 MAC/lease
# markers, the exact-body HTTP GET through `10.0.2.2:18080` and the eth
# counters.
#
# Phase DNS/TOOLS (R7): the same NIC plus the authoritative UDP DNS fixture
# (test.fantuan -> 10.0.2.2) and assert the resolver marker, the ICMP echo
# to the resolved name and the wget marker.  The shell commands (`nslookup`
# with the override server, `ping` and `wget` against the literal address,
# `help`) are fed over the serial console after the boot self-test.
#
# Phase TLS/UDP (R8) and the optional external phase live in
# tools/smoke-net-tls.sh, which this gate calls last; it returns non-zero
# on any offline TLS/UDP failure but never gates external results.
#
# Output: "SMOKE PASS (net loopback)"/"(net slirp)"/"(net dns tools)" and
# the aggregate "SMOKE PASS (net offline gate)"; any failure exits non-zero.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build
export PATH="$HOME/.cargo/bin:$PATH"
# C4: the net gate is explicit — the default .config is the minimal shell.
python3 tools/kconfig.py --profile net >/dev/null \
  || { echo "SMOKE FAIL (net) - profile"; exit 1; }

NET_TIMEOUT="${NET_TIMEOUT:-60}"
DNS_TOOLS_TIMEOUT="${DNS_TOOLS_TIMEOUT:-180}"
HTTP_PORT="${HTTP_PORT:-18080}"
DNS_PORT="${DNS_PORT:-5353}"
FIX_LOG="build/smoke-net-fixtures.log"
FIX_PID=""
cleanup() {
  local p
  for p in "$FIX_PID"; do
    if [ -n "$p" ]; then
      kill "$p" 2>/dev/null || true
      for _ in $(seq 1 30); do
        kill -0 "$p" 2>/dev/null || break
        sleep 0.1
      done
      kill -9 "$p" 2>/dev/null || true
      wait "$p" 2>/dev/null || true
    fi
  done
  FIX_PID=""
  # Gentoo's python-exec launcher can fork the interpreter; pattern-kill the
  # fixtures so no orphan keeps the ports bound.
  pkill -9 -f "tools/net_fixtures.py" 2>/dev/null || true
}
trap cleanup EXIT
# A killed run can leave the stdlib fixture process reparented; clear it so
# the ports are free for this run.
pkill -9 -f "tools/net_fixtures.py" 2>/dev/null || true

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

# Host fixtures (tools/net_fixtures.py): deterministic HTTP body, the
# authoritative DNS answer and the UDP echo server.  The READY lines carry
# the ports and FNV-1a hashes so the gate compares the guest's markers
# exactly.  TLS is added by tools/smoke-net-tls.sh (it needs the per-run CA
# embedded into the kernel build).
start_fixtures() {
  rm -f "$FIX_LOG"
  python3 tools/net_fixtures.py \
    --http-port "$HTTP_PORT" --dns-port "$DNS_PORT" > "$FIX_LOG" 2>&1 &
  FIX_PID=$!
  for _ in $(seq 1 50); do
    grep -q "^READY dns " "$FIX_LOG" 2>/dev/null && break
    sleep 0.1
  done
  if ! grep -q "^READY dns " "$FIX_LOG" 2>/dev/null; then
    echo "SMOKE FAIL (net fixtures) - did not start:"
    cat "$FIX_LOG"
    return 1
  fi
  HTTP_BYTES=$(awk '/^READY http /{print $4}' "$FIX_LOG")
  HTTP_HASH=$(awk '/^READY http /{print $5}' "$FIX_LOG")
}

phase_slirp() {
  local log="build/smoke-net-slirp.log"

  start_fixtures || return 1
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

# R7 offline DNS + tools: the R7 boot markers, and the shell commands fed
# over the serial console after the boot self-test released the network
# clients (running them while the self-test owns the stack perturbs the R5
# loss test).  The feeder paces the bytes (the guest UART FIFO is 16 bytes
# and the kernel polls it) and retries until each command's output marker
# appears.
phase_dns_tools() {
  local log="build/smoke-net-dns-tools.log"
  start_fixtures || return 1
  rm -f "$log"
  ./tools/build.sh >/dev/null 2>&1 || { echo "SMOKE FAIL (net dns tools) - build"; exit 1; }
  local feeder
  feeder=$(cat <<'PY'
import sys, time
log, dns_port, http_port = sys.argv[1], sys.argv[2], sys.argv[3]

def wait(pat, tries):
    for _ in range(tries):
        try:
            with open(log, "rb") as fh:
                if pat in fh.read().decode("utf-8", "replace"):
                    return True
        except FileNotFoundError:
            pass
        time.sleep(0.5)
    return False

def send(line):
    for ch in "\n" + line + "\n":
        sys.stdout.write(ch)
        sys.stdout.flush()
        time.sleep(0.002)

def ensure(cmd, pat):
    for _ in range(8):
        send(cmd)
        if wait(pat, 40):
            return True
    return False

wait("shell: ready", 480)
wait("net: wget ok (url=http://test.fantuan", 480)
time.sleep(2)
ensure("nslookup test.fantuan 10.0.2.2:%s" % dns_port, "nslookup: test.fantuan =>")
ensure("ping 10.0.2.2 1", "ping: 10.0.2.2")
ensure("wget http://10.0.2.2:%s/" % http_port, "wget: http://10.0.2.2")
send("help")
time.sleep(3)
PY
)
  python3 -c "$feeder" "$log" "$DNS_PORT" "$HTTP_PORT" \
    | timeout --signal=KILL "$DNS_TOOLS_TIMEOUT" ./tools/run.sh --net > "$log" 2>&1 || true
  cleanup

  local lease="net: dhcp lease 10.0.2.15/24 gw 10.0.2.2 dns 10.0.2.3"
  local dns="net: dns ok (name=test.fantuan addr=10.0.2.2)"
  local wget="net: wget ok (url=http://test.fantuan:$HTTP_PORT/ bytes=$HTTP_BYTES hash=$HTTP_HASH)"
  local shell_wget="wget: http://10.0.2.2:$HTTP_PORT/ 200 bytes=$HTTP_BYTES hash=$HTTP_HASH"
  if grep -qF "$dns" "$log" \
     && grep -qE "net: ping test.fantuan ok \(seq=[0-9]+ rtt=[0-9]+ ticks\)" "$log" \
     && grep -qF "$wget" "$log" \
     && grep -qF "$lease" "$log" \
     && grep -q "nslookup: test.fantuan => 10.0.2.2" "$log" \
     && grep -qE "ping: 10.0.2.2 \(10.0.2.2\) seq=1 rtt=[0-9]+ ticks" "$log" \
     && grep -qF "$shell_wget" "$log" \
     && grep -qE "^  (ping|nslookup|wget) " "$log" \
     && grep -q "QUERY test.fantuan" "$FIX_LOG" \
     && ! grep -qE "net: (dns|tool) FAILED" "$log"; then
    echo "SMOKE PASS (net dns tools)"
    grep -aE "net: (dns ok|ping test.fantuan|wget ok)" "$log"
    grep -aE "^(nslookup|ping|wget): " "$log"
    return 0
  fi
  echo "SMOKE FAIL (net dns tools) - log tail:"
  tail -40 "$log"
  return 1
}

if ! phase_loopback; then
  exit 1
fi
if ! phase_slirp; then
  exit 1
fi
if ! phase_dns_tools; then
  exit 1
fi
if ! ./tools/smoke-net-tls.sh; then
  exit 1
fi
echo "SMOKE PASS (net offline gate)"
exit 0
