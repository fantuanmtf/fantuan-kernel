#!/usr/bin/env python3
"""tor_relay - host-side TCP->SOCKS5/HTTP relay for the M11 R8 external phase.

The guest (kernel `wget`/external self-test) connects to `10.0.2.2:<port>`,
which SLIRP forwards to this process on 127.0.0.1.  The relay sniffs the
target from the first bytes - TLS ClientHello SNI or the HTTP Host header -
and connects to the local proxy:

  * `--mode socks5` (Tor 9050, I2P 4447): SOCKS5 CONNECT with ATYP=domain,
    so the proxy resolves the name (socks5h semantics) and GFW DNS pollution
    cannot turn a reachable target into a failure;
  * `--mode http` (I2P 4444): HTTP CONNECT through the proxy.

Only TCP is carried: Tor and I2P SOCKS have no UDP ASSOCIATE here, which is
why the offline UDP coverage uses a direct host echo server instead.
Python stdlib only.  Never gates anything: failures are logged.
"""
import argparse
import select
import socket
import struct
import sys

BUF = 16384


def log(message):
    print(message, flush=True)


def parse_sni(data):
    """Return the SNI hostname of a TLS ClientHello, or None."""
    try:
        if len(data) < 45 or data[0] != 0x16 or data[1] != 0x03:
            return None
        if data[5] != 0x01:  # handshake type: client_hello
            return None
        pos = 9 + 2 + 32  # record hdr + version + random
        sid_len = data[pos]
        pos += 1 + sid_len
        cs_len = struct.unpack("!H", data[pos:pos + 2])[0]
        pos += 2 + cs_len
        comp_len = data[pos]
        pos += 1 + comp_len
        ext_total = struct.unpack("!H", data[pos:pos + 2])[0]
        pos += 2
        end = min(pos + ext_total, len(data))
        while pos + 4 <= end:
            etype, elen = struct.unpack("!HH", data[pos:pos + 4])
            pos += 4
            if etype == 0x0000 and elen >= 5:
                # server_name_list: 2-byte list len, 1-byte type, 2-byte len
                nlen = struct.unpack("!H", data[pos + 3:pos + 5])[0]
                return data[pos + 5:pos + 5 + nlen].decode("ascii", "replace")
            pos += elen
    except (IndexError, struct.error):
        pass
    return None


def parse_http_host(data):
    """Return (host, port) from an HTTP/1.x request, or None."""
    try:
        head = data.split(b"\r\n\r\n", 1)[0].decode("latin-1")
    except UnicodeDecodeError:
        return None
    if " HTTP/" not in head.splitlines()[0]:
        return None
    for line in head.splitlines()[1:]:
        if line.lower().startswith("host:"):
            host = line.split(":", 1)[1].strip()
            if ":" in host:
                name, port = host.rsplit(":", 1)
                if port.isdigit():
                    return name, int(port)
            return host, 80
    return None


def recv_exact(sock, n):
    buf = b""
    while len(buf) < n:
        chunk = sock.recv(n - len(buf))
        if not chunk:
            raise ConnectionError("short read")
        buf += chunk
    return buf


def socks5_connect(proxy, host, port):
    s = socket.create_connection(proxy, timeout=15)
    s.sendall(b"\x05\x01\x00")
    if recv_exact(s, 2) != b"\x05\x00":
        raise ConnectionError("socks5 greeting refused")
    name = host.encode("idna")
    req = b"\x05\x01\x00\x03" + bytes([len(name)]) + name + struct.pack("!H", port)
    s.sendall(req)
    head = recv_exact(s, 4)
    if head[1] != 0x00:
        raise ConnectionError("socks5 connect error %d" % head[1])
    atyp = head[3]
    if atyp == 0x01:
        recv_exact(s, 4)
    elif atyp == 0x03:
        recv_exact(s, recv_exact(s, 1)[0])
    elif atyp == 0x04:
        recv_exact(s, 16)
    recv_exact(s, 2)
    return s


def http_connect(proxy, host, port):
    s = socket.create_connection(proxy, timeout=15)
    req = ("CONNECT %s:%d HTTP/1.1\r\nHost: %s:%d\r\n\r\n"
           % (host, port, host, port)).encode("ascii")
    s.sendall(req)
    head = b""
    while b"\r\n\r\n" not in head and len(head) < 8192:
        chunk = s.recv(1024)
        if not chunk:
            raise ConnectionError("http proxy closed")
        head += chunk
    if b" 200 " not in head.split(b"\r\n", 1)[0]:
        raise ConnectionError("http proxy refused: %r" % head[:80])
    rest = head.split(b"\r\n\r\n", 1)[1]
    return s, rest


def pump(client, upstream, first):
    if first:
        upstream.sendall(first)
    while True:
        readable, _, _ = select.select([client, upstream], [], [], 30)
        if not readable:
            break
        for src in readable:
            data = src.recv(BUF)
            if not data:
                return
            (upstream if src is client else client).sendall(data)


def handle(conn, proxy, mode):
    conn.settimeout(10)
    try:
        first = conn.recv(BUF)
    except (socket.timeout, ConnectionError):
        conn.close()
        return
    if not first:
        conn.close()
        return
    host = parse_sni(first)
    port = 443
    if host is None:
        parsed = parse_http_host(first)
        if parsed is None:
            log("RELAY FAIL (no sni/host)")
            conn.close()
            return
        host, port = parsed
    try:
        if mode == "socks5":
            upstream = socks5_connect(proxy, host, port)
            rest = b""
        else:
            upstream, rest = http_connect(proxy, host, port)
        log("RELAY %s:%d via %s" % (host, port, mode))
        pump(conn, upstream, rest + first)
    except (OSError, ConnectionError) as exc:
        log("RELAY FAIL %s:%d (%s)" % (host, port, exc))
    finally:
        for s in (conn, locals().get("upstream")):
            try:
                s.close()
            except (OSError, AttributeError):
                pass


def main(argv):
    parser = argparse.ArgumentParser(prog="tools/tor_relay.py")
    parser.add_argument("--listen", default="127.0.0.1:19050")
    parser.add_argument("--proxy", default="127.0.0.1:9050")
    parser.add_argument("--mode", choices=["socks5", "http"], default="socks5")
    args = parser.parse_args(argv)
    lhost, lport = args.listen.rsplit(":", 1)
    phost, pport = args.proxy.rsplit(":", 1)
    proxy = (phost, int(pport))
    srv = socket.socket()
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((lhost, int(lport)))
    srv.listen(8)
    log("RELAY listening on %s -> %s (%s)" % (args.listen, args.proxy, args.mode))
    while True:
        try:
            conn, _ = srv.accept()
        except OSError as exc:
            log("RELAY accept failed (%s)" % exc)
            continue
        handle(conn, proxy, args.mode)


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except KeyboardInterrupt:
        pass
