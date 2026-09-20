#!/usr/bin/env bash
# aarch64 smoke (M11 R9a/R9b).
#
# Phase R9a: direct-FDT boot on QEMU `virt`. One bounded run: boot the raw
# kernel image (QEMU's Linux-compatible protocol passes the DTB in x0),
# assert the FDT/MMU/GIC/timer/task markers, the deliberate BRK resume, then
# drive `help`/`bootinfo` over the serial shell. Negative checks: no
# unexpected traps, no panic, no heartbeat after the shell.
#
# Phase NET (R9b): the same direct-FDT machine plus the polled virtio-net
# MMIO device on QEMU user networking (`-device virtio-net-device`,
# `-global virtio-mmio.force-legacy=false` - QEMU virt has no PCI for the
# e1000, and the modern transport is what the driver binds). The offline
# fixtures from tools/net_fixtures.py run on the host (HTTP 18080, DNS 5353,
# TLS 18443, UDP 18082, per-run pinned CA in build/smoke-net-tls); with
# FANTUAN_NET_FIXTURES=1 the boot self-test runs the full marker set and the
# shell commands are fed afterwards. Asserts: virtio-net up, DHCP lease, the
# IPv4/loopback suite (rump self-test included), HTTP/DNS/ping/wget, the TLS
# KATs + HTTPS GET + host UDP echo, the external skip, and the shell
# transcripts for nslookup/ping/wget.
#
# Skips (clearly) when qemu-system-aarch64 is absent; fails when the local
# QEMU lacks the SLIRP backend (needed for the network phase).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v qemu-system-aarch64 >/dev/null 2>&1; then
  echo "SMOKE SKIP (aarch64): qemu-system-aarch64 not installed"
  exit 0
fi

HTTP_PORT="${HTTP_PORT:-18080}"
DNS_PORT="${DNS_PORT:-5353}"
TLS_PORT="${TLS_PORT:-18443}"
UDP_PORT="${UDP_PORT:-18082}"
NET_TIMEOUT="${AARCH64_NET_TIMEOUT:-240}"
TLS_DIR="build/smoke-net-tls"
FIX_LOG="build/smoke-aarch64-fixtures.log"
FIX_PID=""
cleanup() {
  if [ -n "$FIX_PID" ]; then
    kill "$FIX_PID" 2>/dev/null || true
    kill -9 "$FIX_PID" 2>/dev/null || true
    wait "$FIX_PID" 2>/dev/null || true
  fi
  FIX_PID=""
  pkill -9 -f "tools/net_fixtures.py" 2>/dev/null || true
}
trap cleanup EXIT
pkill -9 -f "tools/net_fixtures.py" 2>/dev/null || true

# --- phase R9a: minimal-profile direct FDT boot --------------------------
phase_r9a() {
  local log="build/smoke-aarch64.log"
  # R9a is a minimal-profile bring-up: the shell table is help/bootinfo only.
  python3 tools/kconfig.py --profile minimal >/dev/null || return 1
  rm -f "$log"
  # Feed the shell through the serial console (the pipe stays open so QEMU
  # does not see EOF); the run is bounded by the timeout.
  (sleep 4; printf 'help\nbootinfo\n'; sleep 8) \
    | timeout --signal=KILL 30 ./tools/run.sh --arch aarch64 > "$log" 2>&1 || true

  # C4: the heartbeat stops once the shell owns the console.
  local ticks
  ticks=$(awk '/shell: ready/{seen=1} seen && /^tick: /{n++} END{print n+0}' "$log")

  if grep -q "fantuan v0.0.3 (aarch64) - QEMU virt" "$log" \
     && grep -q "boot: EL1, dtb=0x" "$log" \
     && grep -q "uart: pl011 up" "$log" \
     && grep -q "fdt: memory 0x40000000" "$log" \
     && grep -q "fdt: model=linux,dummy-virt" "$log" \
     && grep -q "fdt: uart=0x9000000 (pl011)" "$log" \
     && grep -qE "mm: frame allocator ready: [0-9]+ MiB usable" "$log" \
     && grep -q "mm: frame self-test ok (via direct map)" "$log" \
     && grep -q "mmu: 4K granule, direct map at 0xffff000000000000" "$log" \
     && grep -q "intc: GICv2 up" "$log" \
     && grep -q "timer: 100 Hz" "$log" \
     && grep -q "trap: brk handled" "$log" \
     && grep -q "exc: resumed after brk" "$log" \
     && grep -q "sched: 2 aarch64 kernel tasks spawned" "$log" \
     && grep -q "task 1 (tid 1): hello 0" "$log" \
     && grep -q "task 2 (tid 2): hello 0" "$log" \
     && grep -q "shell: ready (root@Fantuan-MTF" "$log" \
     && grep -q "root@Fantuan-MTF> " "$log" \
     && grep -q "shell commands (root@Fantuan-MTF" "$log" \
     && grep -q "this table" "$log" \
     && grep -q "boot handover details" "$log" \
     && grep -q "kernel_base 0x40080000" "$log" \
     && [ "$ticks" -eq 0 ] \
     && ! grep -q "trap: unexpected" "$log" \
     && ! grep -q "fdt: parse failed" "$log" \
     && ! grep -q "PANIC" "$log"; then
    echo "SMOKE PASS (aarch64 R9a: direct FDT boot, PL011, 4K MMU + direct map, GICv2, 100 Hz, BRK resume, tasks, shell)"
    grep -aE "fantuan v0.0.3 \(aarch64|boot: |uart: |fdt: |cpu: |mm: |mmu: |intc: |timer: |trap: |exc: |sched: |task [12] |shell|help|bootinfo|kernel_base" "$log" | head -40 || true
    return 0
  fi
  echo "SMOKE FAIL (aarch64 R9a) - log tail:"
  tail -30 "$log"
  return 1
}

# --- phase NET: virtio-net MMIO + the offline network gate ---------------
generate_ca() {
  rm -rf "$TLS_DIR"
  mkdir -p "$TLS_DIR"
  command -v openssl >/dev/null 2>&1 || {
    echo "SMOKE FAIL (aarch64 net) - openssl not found"
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
  ) || { echo "SMOKE FAIL (aarch64 net) - CA generation"; return 1; }
}

start_fixtures() {
  rm -f "$FIX_LOG"
  python3 tools/net_fixtures.py \
    --http-port "$HTTP_PORT" --dns-port "$DNS_PORT" \
    --tls-port "$TLS_PORT" --udp-port "$UDP_PORT" \
    --cert "$TLS_DIR/srv.pem" --key "$TLS_DIR/srv.key" > "$FIX_LOG" 2>&1 &
  FIX_PID=$!
  for _ in $(seq 1 50); do
    grep -q "^READY tls " "$FIX_LOG" 2>/dev/null && break
    sleep 0.1
  done
  if ! grep -q "^READY tls " "$FIX_LOG" 2>/dev/null; then
    echo "SMOKE FAIL (aarch64 net) - fixtures did not start:"
    cat "$FIX_LOG"
    return 1
  fi
  HTTP_BYTES=$(awk '/^READY http /{print $4}' "$FIX_LOG")
  HTTP_HASH=$(awk '/^READY http /{print $5}' "$FIX_LOG")
  TLS_BYTES=$(awk '/^READY tls /{print $4}' "$FIX_LOG")
  TLS_HASH=$(awk '/^READY tls /{print $5}' "$FIX_LOG")
}

phase_net() {
  local log="build/smoke-aarch64-net.log"
  python3 tools/kconfig.py --profile tls >/dev/null || return 1
  generate_ca || return 1
  start_fixtures || return 1
  rm -f "$log"
  export FANTUAN_NET_FIXTURES=1
  ./tools/build.sh --arch aarch64 >/dev/null 2>&1 \
    || { echo "SMOKE FAIL (aarch64 net) - build"; return 1; }

  # Paced serial feeder: wait for the boot self-test to release the clients,
  # then type the tool commands and check each transcript marker.
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
    try:
        for ch in "\n" + line + "\n":
            sys.stdout.write(ch)
            sys.stdout.flush()
            time.sleep(0.002)
    except BrokenPipeError:
        pass

def ensure(cmd, pat):
    for _ in range(6):
        send(cmd)
        if wait(pat, 30):
            return True
    return False

wait("net: ext skip", 480) or wait("net: ext done", 480)
time.sleep(1)
ensure("nslookup test.fantuan 10.0.2.2:%s" % dns_port, "nslookup: test.fantuan =>")
ensure("ping 10.0.2.2 1", "ping: 10.0.2.2")
ensure("wget http://10.0.2.2:%s/" % http_port, "wget: http://10.0.2.2")
send("help")
time.sleep(2)
PY
)
  FANTUAN_NET_FIXTURES=1 python3 -c "$feeder" "$log" "$DNS_PORT" "$HTTP_PORT" \
    | timeout --signal=KILL "$NET_TIMEOUT" ./tools/run.sh --arch aarch64 --net \
        > "$log" 2>&1 || true
  cleanup

  local lease="net: dhcp lease 10.0.2.15/24 gw 10.0.2.2 dns 10.0.2.3"
  local http="net: http get ok (url=http://10.0.2.2:$HTTP_PORT/ bytes=$HTTP_BYTES hash=$HTTP_HASH)"
  local dns="net: dns ok (name=test.fantuan addr=10.0.2.2)"
  local wget="net: wget ok (url=http://test.fantuan:$HTTP_PORT/ bytes=$HTTP_BYTES hash=$HTTP_HASH)"
  local https="net: https get ok (url=https://test.fantuan:$TLS_PORT/ bytes=$TLS_BYTES hash=$TLS_HASH)"
  local shell_wget="wget: http://10.0.2.2:$HTTP_PORT/ 200 bytes=$HTTP_BYTES hash=$HTTP_HASH"
  if grep -qE "net: virtio-net up mac=([0-9a-f]{2}:){5}[0-9a-f]{2}" "$log" \
     && grep -q "net: lo0 up 127.0.0.1/8" "$log" \
     && grep -q "rump: mbuf self-test ok" "$log" \
     && grep -q "rump: callout self-test ok (fires=20)" "$log" \
     && grep -q "net: tcp transfer ok (bytes=65536 hash=ff8ebd03)" "$log" \
     && grep -qE "net: tcp retransmit ok \(drops=[0-9]+ retrans=[0-9]+\)" "$log" \
     && grep -qF "$lease" "$log" \
     && grep -qF "$http" "$log" \
     && grep -qF "$dns" "$log" \
     && grep -qE "net: ping test.fantuan ok \(seq=[0-9]+ rtt=[0-9]+ ticks\)" "$log" \
     && grep -qF "$wget" "$log" \
     && grep -q "tls: KATs ok (sha256 + aes-gcm + rsa)" "$log" \
     && grep -qF "$https" "$log" \
     && grep -qF "net: udp host ok (tx=4 rx=4 bytes=1024)" "$log" \
     && grep -q "net: ext skip (no relay)" "$log" \
     && grep -q "nslookup: test.fantuan => 10.0.2.2" "$log" \
     && grep -qE "ping: 10.0.2.2 \(10.0.2.2\) seq=1 rtt=[0-9]+ ticks" "$log" \
     && grep -qF "$shell_wget" "$log" \
     && grep -q "QUERY test.fantuan" "$FIX_LOG" \
     && grep -qE "^HTTP GET / " "$FIX_LOG" \
     && grep -qE "^TLS GET / " "$FIX_LOG" \
     && [ "$(grep -c '^UDP echo ' "$FIX_LOG")" -ge 4 ] \
     && ! grep -q "PANIC" "$log" \
     && ! grep -q "trap: unexpected" "$log" \
     && ! grep -q "net: ip4 FAILED" "$log" \
     && ! grep -q "tls: FAILED" "$log"; then
    echo "SMOKE PASS (aarch64 R9b: virtio-net MMIO + DHCP + IPv4/TCP + HTTP/DNS/wget + TLS/HTTPS/UDP + shell tools)"
    grep -aE "net: (virtio-net up|lo0 up|dhcp lease|http get ok|dns ok|ping test|wget ok|https get ok|udp host ok|ext skip)" "$log"
    grep -aE "^(nslookup|ping|wget): " "$log"
    return 0
  fi
  echo "SMOKE FAIL (aarch64 net) - log tail:"
  tail -40 "$log"
  return 1
}

if ! phase_r9a; then
  exit 1
fi
if ! command -v openssl >/dev/null 2>&1; then
  echo "SMOKE SKIP (aarch64 net): openssl not installed"
  exit 0
fi
if ! qemu-system-aarch64 -machine virt -netdev help 2>&1 | grep -q "^user$"; then
  echo "SMOKE SKIP (aarch64 net): this QEMU binary has no SLIRP 'user' backend"
  exit 0
fi
if ! phase_net; then
  exit 1
fi
echo "SMOKE PASS (aarch64: R9a direct FDT boot + R9b virtio-net/TLS)"
exit 0
