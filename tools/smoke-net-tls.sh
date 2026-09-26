#!/usr/bin/env bash
# M11 R8 gate: offline TLS/HTTPS + host UDP echo, then the optional external
# phase (Tor/I2P through tools/tor_relay.py).  Called by tools/smoke-net.sh.
#
# Phase TLS/UDP: generate a per-run self-signed CA and server certificate
# under build/smoke-net-tls/ (SAN test.fantuan + IP 10.0.2.2), start the
# fixtures with TLS on 127.0.0.1:18443, build the kernel with
# FANTUAN_NET_FIXTURES=1 (so build.rs embeds ca.der and enables the R8
# tests), boot with the e1000 and assert:
#   tls: KATs ok (sha256 + aes-gcm + rsa)
#   net: https get ok (url=https://test.fantuan:18443/ ...)  (pinned CA)
#   net: udp host ok (tx=4 rx=4 bytes=1024)
#   net: ext skip (no relay)                                 (deterministic)
# plus a shell `wget https://10.0.2.2:18443/` transcript.
#
# Phase EXTERNAL: probe 9050 (Tor SOCKS5), then 4447 (I2P SOCKS5), then 4444
# (I2P HTTP); start the relay for whichever answers and boot.  Results are
# recorded per rung and never gate: without a proxy the phase prints SKIP and
# returns 0.
#
#   SMOKE_NET_SKIP_OFFLINE=1  run only the external phase (debug/iteration)
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p build
export PATH="$HOME/.cargo/bin:$PATH"
# The R8 gate is the TLS profile: net + TOOLS + NET_DRIVERS + TLS.
python3 tools/kconfig.py --profile tls >/dev/null \
  || { echo "SMOKE FAIL (net tls) - profile"; exit 1; }

TLS_DIR="build/smoke-net-tls"
FIX_LOG="build/smoke-net-fixtures.log"
RELAY_LOG="build/smoke-net-relay.log"
TLS_LOG="build/smoke-net-tls-udp.log"
EXT_LOG="build/smoke-net-external.log"
TLS_TIMEOUT="${TLS_TIMEOUT:-240}"
EXT_TIMEOUT="${EXT_TIMEOUT:-600}"
HTTP_PORT="${HTTP_PORT:-18080}"
DNS_PORT="${DNS_PORT:-5353}"
TLS_PORT="${TLS_PORT:-18443}"
UDP_PORT="${UDP_PORT:-18082}"
RELAY_PORT="${RELAY_PORT:-19050}"
FIX_PID=""
RELAY_PID=""
cleanup() {
  local p
  for p in "$FIX_PID" "$RELAY_PID"; do
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
  RELAY_PID=""
  # Gentoo's python-exec launcher can fork the interpreter; pattern-kill the
  # fixtures so no orphan keeps the ports bound.
  pkill -9 -f "tools/net_fixtures.py" 2>/dev/null || true
  pkill -9 -f "tools/tor_relay.py" 2>/dev/null || true
}
trap cleanup EXIT
# A killed run can leave the fixtures reparented; clear them so the ports
# are free for this run (the relay only ever uses RELAY_PORT).
pkill -9 -f "tools/net_fixtures.py" 2>/dev/null || true
pkill -9 -f "tools/tor_relay.py" 2>/dev/null || true

# Generate the per-run CA + server certificate and embeddable DER.  No
# validity window is checked by the kernel (no RTC): the pinned DER is the
# trust anchor and the SAN/hostname check is what the gate exercises.
generate_ca() {
  rm -rf "$TLS_DIR"
  mkdir -p "$TLS_DIR"
  command -v openssl >/dev/null 2>&1 || {
    echo "SMOKE FAIL (net tls) - openssl not found"
    return 1
  }
  cat > "$TLS_DIR/san.cnf" <<'EOF'
subjectAltName=DNS:test.fantuan,DNS:localhost,IP:10.0.2.2
basicConstraints=CA:FALSE
keyUsage=digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
EOF
  (
    cd "$TLS_DIR"
    openssl genrsa -out ca.key 2048 >/dev/null 2>&1 &&
    openssl req -x509 -new -key ca.key -sha256 -days 2 \
      -subj "/CN=fantuan-smoke-ca" -out ca.pem >/dev/null 2>&1 &&
    openssl genrsa -out srv.key 2048 >/dev/null 2>&1 &&
    openssl req -new -key srv.key -subj "/CN=test.fantuan" \
      -out srv.csr >/dev/null 2>&1 &&
    openssl x509 -req -in srv.csr -CA ca.pem -CAkey ca.key -CAcreateserial \
      -days 2 -sha256 -extfile san.cnf -out srv.pem >/dev/null 2>&1 &&
    openssl x509 -in ca.pem -outform DER -out ca.der
  ) || { echo "SMOKE FAIL (net tls) - CA generation"; return 1; }
}

start_tls_fixtures() {
  generate_ca || return 1
  rm -f "$FIX_LOG"
  python3 tools/net_fixtures.py \
    --http-port "$HTTP_PORT" --dns-port "$DNS_PORT" \
    --tls-port "$TLS_PORT" --udp-port "$UDP_PORT" \
    --cert "$TLS_DIR/srv.pem" --key "$TLS_DIR/srv.key" > "$FIX_LOG" 2>&1 &
  FIX_PID=$!
  for _ in $(seq 1 50); do
    grep -q "^READY udp " "$FIX_LOG" 2>/dev/null && break
    sleep 0.1
  done
  if ! grep -q "^READY tls " "$FIX_LOG" 2>/dev/null; then
    echo "SMOKE FAIL (net tls) - fixtures did not start:"
    cat "$FIX_LOG"
    return 1
  fi
  HTTP_BYTES=$(awk '/^READY http /{print $4}' "$FIX_LOG")
  HTTP_HASH=$(awk '/^READY http /{print $5}' "$FIX_LOG")
  TLS_BYTES=$(awk '/^READY tls /{print $4}' "$FIX_LOG")
  TLS_HASH=$(awk '/^READY tls /{print $5}' "$FIX_LOG")
}

phase_tls_udp() {
  start_tls_fixtures || return 1
  rm -f "$TLS_LOG"
  # tools/run.sh rebuilds the kernel itself, so the fixture switch must be
  # exported for the whole phase (build.sh + run.sh).
  export FANTUAN_NET_FIXTURES=1
  ./tools/build.sh >/dev/null 2>&1 \
    || { echo "SMOKE FAIL (net tls udp) - build"; return 1; }
  local feeder
  feeder=$(cat <<'PY'
import sys, time
log, tls_port = sys.argv[1], sys.argv[2]

def wait(pat, tries):
    for _ in range(tries):
        try:
            with open(log, "rb") as fh:
                text = fh.read().decode("utf-8", "replace")
            if any(p in text for p in pat):
                return True
        except FileNotFoundError:
            pass
        time.sleep(0.5)
    return False

def send(line):
    try:
        for ch in "\n" + line + "\n":
            sys.stdout.write(ch)
            sys.stdout.flush()
            time.sleep(0.002)
    except BrokenPipeError:
        pass

wait(("net: ext skip", "net: ext done"), 400)
time.sleep(1)
# P4: the console lands in the login shell (bash); the HTTPS tool lives in the
# kernel (CONFIG_TLS), so drop to the built-in shell before driving it.
send("exit")
time.sleep(2)
send("wget https://10.0.2.2:%s/" % tls_port)
time.sleep(4)
PY
)
  FANTUAN_NET_FIXTURES=1 python3 -c "$feeder" "$TLS_LOG" "$TLS_PORT" \
    | timeout --signal=KILL "$TLS_TIMEOUT" ./tools/run.sh --net \
        > "$TLS_LOG" 2>&1 || true
  cleanup

  local https="net: https get ok (url=https://test.fantuan:$TLS_PORT/ bytes=$TLS_BYTES hash=$TLS_HASH)"
  local udp="net: udp host ok (tx=4 rx=4 bytes=1024)"
  local shell="wget: https://10.0.2.2:$TLS_PORT/ 200 bytes=$TLS_BYTES hash=$TLS_HASH"
  if grep -q "tls: KATs ok (sha256 + aes-gcm + rsa)" "$TLS_LOG" \
     && grep -qF "$https" "$TLS_LOG" \
     && grep -qF "$udp" "$TLS_LOG" \
     && grep -q "net: ext skip (no relay)" "$TLS_LOG" \
     && grep -qF "$shell" "$TLS_LOG" \
     && grep -qE "^TLS GET / " "$FIX_LOG" \
     && [ "$(grep -c '^UDP echo ' "$FIX_LOG")" -ge 4 ] \
     && ! grep -q "tls: FAILED" "$TLS_LOG" \
     && ! grep -q "net: udp host FAILED" "$TLS_LOG"; then
    echo "SMOKE PASS (net tls udp)"
    grep -aE "tls: KATs|net: https get|net: udp host|net: ext skip" "$TLS_LOG"
    grep -aE "^wget: https" "$TLS_LOG"
    return 0
  fi
  echo "SMOKE FAIL (net tls udp) - log tail:"
  tail -40 "$TLS_LOG"
  return 1
}

# "port mode" of the first reachable local proxy, or empty.
detect_proxy() {
  python3 - <<'PY'
import socket
for port, mode in ((9050, "socks5"), (4447, "socks5"), (4444, "http")):
    s = socket.socket()
    s.settimeout(0.4)
    try:
        s.connect(("127.0.0.1", port))
        print("%d %s" % (port, mode))
        break
    except OSError:
        pass
    finally:
        s.close()
PY
}

# Record per-rung external results; never a gate.
report_external() {
  local log="$1" label
  for label in github x ddg duckai; do
    if grep -q "net: ext $label ok" "$log"; then
      echo "SMOKE EXTERNAL $label PASS"
    elif grep -q "net: ext $label best-effort" "$log"; then
      echo "SMOKE EXTERNAL $label BEST-EFFORT (recorded)"
    elif grep -q "net: ext $label FAIL" "$log"; then
      echo "SMOKE EXTERNAL $label FAIL (recorded)"
    else
      echo "SMOKE EXTERNAL $label SKIP"
    fi
  done
  grep -aE "^net: ext (skip|done)" "$log" | sed 's/^/  /' || true
}

phase_external() {
  local detected port mode
  detected="$(detect_proxy)"
  if [ -z "$detected" ]; then
    echo "SMOKE SKIP (net external: no proxy on 127.0.0.1:9050/4447/4444)"
    return 0
  fi
  port="${detected%% *}"
  mode="${detected##* }"
  echo "SMOKE (net external): proxy 127.0.0.1:$port mode=$mode"
  start_tls_fixtures || return 0
  rm -f "$RELAY_LOG" "$EXT_LOG"
  python3 tools/tor_relay.py --listen "127.0.0.1:$RELAY_PORT" \
    --proxy "127.0.0.1:$port" --mode "$mode" > "$RELAY_LOG" 2>&1 &
  RELAY_PID=$!
  for _ in $(seq 1 30); do
    grep -q "RELAY listening" "$RELAY_LOG" 2>/dev/null && break
    sleep 0.1
  done
  FANTUAN_NET_FIXTURES=1 ./tools/build.sh >/dev/null 2>&1 || return 0
  FANTUAN_NET_FIXTURES=1 timeout --signal=KILL "$EXT_TIMEOUT" \
    ./tools/run.sh --net > "$EXT_LOG" 2>&1 || true
  cleanup
  echo "SMOKE EXTERNAL results (never gated):"
  report_external "$EXT_LOG"
  grep -aE "^RELAY " "$RELAY_LOG" | head -12 | sed 's/^/  /' || true
  return 0
}

if [ "${SMOKE_NET_SKIP_OFFLINE:-0}" != "1" ]; then
  if ! phase_tls_udp; then
    exit 1
  fi
fi
phase_external
exit 0
