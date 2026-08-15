#!/usr/bin/env python3
"""Send a screendump command to a QEMU HMP monitor over TCP."""
import socket
import sys
import time

port = int(sys.argv[1]) if len(sys.argv) > 1 else 4555
path = sys.argv[2] if len(sys.argv) > 2 else "build/screen.png"

s = socket.create_connection(("127.0.0.1", port), timeout=5)
time.sleep(0.3)
try:
    s.recv(4096)
except Exception:
    pass
s.sendall(f"screendump {path}\n".encode())
time.sleep(2)
s.sendall(b"quit\n")
s.close()
print(f"dumped {path}")
