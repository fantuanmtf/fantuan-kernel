#!/usr/bin/env python3
"""net_fixtures - host-side fixtures for tools/smoke-net.sh (M11 R6-R8).

One stdlib process serves everything the guest reaches as 10.0.2.2:

  HTTP  18080  deterministic body, FNV-1a hash printed on READY
  DNS   5353   authoritative test.fantuan -> 10.0.2.2, NXDOMAIN otherwise
  TLS   18443  python ssl, TLS 1.2 only, cert/key passed by the smoke
  UDP   18082  echo server (the UDP coverage Tor/I2P SOCKS cannot carry)

READY lines carry the ports and body hashes so the shell gates compare the
guest's markers exactly.  Every request/query/echo is logged as evidence.
"""
import argparse
import http.server
import socket
import socketserver
import ssl
import struct
import sys
import threading

HTTP_BODY = b"fantuan-r6-slirp-body\n" * 64
TLS_BODY = b"fantuan-r8-tls-body\n" * 48


def fnv1a(data):
    h = 2166136261
    for b in data:
        h = ((h ^ b) * 16777619) & 0xFFFFFFFF
    return h


def log(message):
    print(message, flush=True)


def serve_http(port):
    body = HTTP_BODY

    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.0"

        def do_GET(self):
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            log("HTTP GET %s 200 bytes=%d" % (self.path, len(body)))

        def log_message(self, fmt, *args):
            pass

    class Server(socketserver.ThreadingTCPServer):
        allow_reuse_address = True

    srv = Server(("127.0.0.1", port), Handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv


def serve_dns(port):
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(("127.0.0.1", port))

    def qname(query, off):
        labels = []
        while query[off] != 0:
            n = query[off]
            labels.append(query[off + 1:off + 1 + n])
            off += 1 + n
        return b".".join(labels).lower(), off + 1

    def loop():
        while True:
            data, peer = sock.recvfrom(512)
            if len(data) < 12:
                continue
            name, off = qname(data, 12)
            if off + 4 > len(data):
                continue
            log("DNS QUERY %s" % name.decode("ascii", "replace"))
            question = data[12:off + 4]
            tid = data[0:2]
            if name == b"test.fantuan":
                answer = (b"\xc0\x0c" + struct.pack("!HHIH", 1, 1, 60, 4)
                          + socket.inet_aton("10.0.2.2"))
                resp = tid + b"\x81\x80" + struct.pack("!HHHH", 1, 1, 0, 0) + question + answer
            else:
                resp = tid + b"\x81\x83" + struct.pack("!HHHH", 1, 0, 0, 0) + question
            sock.sendto(resp, peer)

    threading.Thread(target=loop, daemon=True).start()
    return sock


def serve_tls(port, cert, key):
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.load_cert_chain(cert, key)
    ctx.minimum_version = ssl.TLSVersion.TLSv1_2
    ctx.maximum_version = ssl.TLSVersion.TLSv1_2
    srv = socket.socket()
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", port))
    srv.listen(4)

    def loop():
        while True:
            conn, _ = srv.accept()
            try:
                tls = ctx.wrap_socket(conn, server_side=True)
                request = b""
                while b"\r\n\r\n" not in request and len(request) < 16384:
                    request += tls.recv(4096)
                line = request.split(b"\r\n", 1)[0].decode("latin-1", "replace")
                head = ("HTTP/1.0 200 OK\r\nContent-Type: application/octet-stream\r\n"
                        "Content-Length: %d\r\nConnection: close\r\n\r\n" % len(TLS_BODY))
                tls.sendall(head.encode() + TLS_BODY)
                log("TLS %s ciphers=%s bytes=%d" % (line, tls.cipher()[0], len(TLS_BODY)))
            except (ssl.SSLError, OSError) as exc:
                log("TLS ERR %s" % exc)
            finally:
                try:
                    conn.close()
                except OSError:
                    pass

    threading.Thread(target=loop, daemon=True).start()
    return srv


def serve_udp(port):
    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    sock.bind(("127.0.0.1", port))
    counter = [0]

    def loop():
        while True:
            data, peer = sock.recvfrom(2048)
            counter[0] += 1
            sock.sendto(data, peer)
            log("UDP echo %d bytes=%d" % (counter[0], len(data)))

    threading.Thread(target=loop, daemon=True).start()
    return sock


def main(argv):
    parser = argparse.ArgumentParser(prog="tools/net_fixtures.py")
    parser.add_argument("--http-port", type=int, default=18080)
    parser.add_argument("--dns-port", type=int, default=5353)
    parser.add_argument("--tls-port", type=int, default=18443)
    parser.add_argument("--udp-port", type=int, default=18082)
    parser.add_argument("--cert")
    parser.add_argument("--key")
    args = parser.parse_args(argv)
    servers = [
        ("http", args.http_port, serve_http, (args.http_port,)),
        ("dns", args.dns_port, serve_dns, (args.dns_port,)),
        ("udp", args.udp_port, serve_udp, (args.udp_port,)),
    ]
    tls_ready = bool(args.cert and args.key)
    if tls_ready:
        servers.append(("tls", args.tls_port, serve_tls,
                        (args.tls_port, args.cert, args.key)))
    for label, port, fn, call_args in servers:
        try:
            fn(*call_args)
        except OSError as exc:
            log("FIXTURES FAILED (%s %d: %s)" % (label, port, exc))
            return 1
    log("READY http %d %d %08x" % (args.http_port, len(HTTP_BODY), fnv1a(HTTP_BODY)))
    if tls_ready:
        log("READY tls %d %d %08x" % (args.tls_port, len(TLS_BODY), fnv1a(TLS_BODY)))
    log("READY dns %d" % args.dns_port)
    log("READY udp %d" % args.udp_port)
    threading.Event().wait()
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
