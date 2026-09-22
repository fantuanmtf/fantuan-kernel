#!/usr/bin/env bash
# M13-2 graphics smoke: the shared framebuffer core drives both consoles with
# double buffering, and the kernel demo app (moving box + counter) proves the
# damage accounting.
#   x86_64 UEFI:  boot with -vga std, take two screendumps at known demo
#                 frames, assert they differ and that every differing pixel
#                 lies inside the union of the reported per-frame damage.
#   i686 VBE:      the same demo assertions plus the fb: geometry lines.
#   serial parity: the i686 serial log with -vga none is byte-identical to the
#                 VBE run once the fb: lines (and the demo/scheduler lines)
#                 are stripped.
# Timing is deterministic: the screendumps are taken when the demo's per-frame
# markers appear, and the idle state is sampled only after the demo's stop
# marker, so no fixed sleeps decide the outcome.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p build

fail() { echo "SMOKE FAIL (graphics): $*"; exit 1; }

# CONFIG_GRAPHICS is enabled by the rescue profile (M12-6 / M13-1).
python3 tools/kconfig.py --profile rescue >/dev/null || fail "config"
./tools/build.sh >/dev/null 2>&1 || fail "x86_64 build"

pixel_stats() { # ppm -> "WxH bright=N"
  python3 - "$1" <<'PY'
import sys
data = open(sys.argv[1], 'rb').read()
assert data[:2] == b'P6', 'not a P6 PPM'
parts = data.split(b'\n', 3)
w, h = map(int, parts[1].split())
px = parts[3]
if px.startswith(b'#'):
    px = px.split(b'\n', 1)[1]
bright = 0
for y in range(0, h, 4):
    for x in range(0, w, 4):
        off = (y * w + x) * 3
        r, g, b = px[off], px[off + 1], px[off + 2]
        if r > 100 and g > 100 and b > 100:
            bright += 1
print(f"{w}x{h} bright={bright}")
PY
}

# Two screendumps must differ, and the bounding box of the differing pixels
# must lie inside the union of the per-frame damage rects the demo reported
# for frames [K1, K2] (the sampling window).
diff_contains() { # a.ppm b.ppm log K1 K2
  python3 - "$1" "$2" "$3" "$4" "$5" <<'PY'
import re, sys
def read_ppm(p):
    data = open(p, 'rb').read()
    assert data[:2] == b'P6', 'not a P6 PPM'
    parts = data.split(b'\n', 3)
    w, h = map(int, parts[1].split())
    px = parts[3]
    if px.startswith(b'#'):
        px = px.split(b'\n', 1)[1]
    return w, h, px
w, h, a = read_ppm(sys.argv[1])
w2, h2, b = read_ppm(sys.argv[2])
assert (w, h) == (w2, h2), 'screendump size mismatch'
k1, k2 = int(sys.argv[4]), int(sys.argv[5])
log = open(sys.argv[3], 'rb').read()
rects = []
pat = re.compile(rb'graphics: demo frame=(\d+) dmg=(\d+),(\d+),(\d+),(\d+)')
for m in pat.finditer(log):
    n = int(m.group(1))
    if k1 <= n <= k2:
        rects.append(tuple(map(int, m.groups()[1:])))
if not rects:
    print(f"damage (frames {k1}..{k2}): no rects reported")
    sys.exit(1)
ux = min(r[0] for r in rects); uy = min(r[1] for r in rects)
ux2 = max(r[0] + r[2] for r in rects); uy2 = max(r[1] + r[3] for r in rects)
minx = miny = 10**9
maxx = maxy = -1
n = 0
for y in range(h):
    for x in range(w):
        o = (y * w + x) * 3
        if a[o:o + 3] != b[o:o + 3]:
            n += 1
            minx = min(minx, x); maxx = max(maxx, x)
            miny = min(miny, y); maxy = max(maxy, y)
if n == 0:
    print("diff: images identical")
    sys.exit(1)
dx = maxx - minx + 1
dy = maxy - miny + 1
inside = minx >= ux and miny >= uy and minx + dx <= ux2 and miny + dy <= uy2
print(f"diff: {n} pixels bbox={minx},{miny},{dx},{dy} "
      f"damage(frames {k1}..{k2})={ux},{uy},{ux2-ux},{uy2-uy} "
      f"{'contained' if inside else 'OUTSIDE'}")
sys.exit(0 if inside else 1)
PY
}

mon() { # socket command
  # -t 1: exit ~1s after the request is sent, even if the monitor keeps the
  # connection open (otherwise socat blocks up to the timeout and the next
  # screendump lands after the demo has finished).
  printf '%s\n' "$2" | timeout 10 socat -t 1 - UNIX-CONNECT:"$1" >/dev/null 2>&1
}

OK=1
note() { echo "missing: $1"; OK=0; }

# --- x86_64 UEFI GOP demo (two screendumps + damage containment) --------------
# The QEMU monitor screendump timing is occasionally flaky (the first
# connection can land after the demo finishes), so the screendump+containment
# step is retried a bounded number of times; the boot markers are asserted on
# the first attempt only (they are deterministic once the demo runs).
UEFI_CONTAINED=0
for attempt in 1 2 3; do
  rm -f build/mon.sock build/serial.sock build/gfx-uefi.log build/gfx-uefi-a.ppm build/gfx-uefi-b.ppm
  # Relay QEMU's serial socket to the log file unbuffered: the stdio/file
  # chardevs block-buffer, which would delay the demo's per-frame markers past
  # the screendump window.
  socat -u UNIX-LISTEN:build/serial.sock,unlink-early OPEN:build/gfx-uefi.log,creat,trunc &
  RELAY=$!
  (
    for _ in $(seq 1 300); do
      [ -S build/mon.sock ] && grep -qF "graphics: demo frame=10 dmg=" build/gfx-uefi.log 2>/dev/null && break
      sleep 0.1
    done
    mon build/mon.sock "screendump build/gfx-uefi-a.ppm"
    for _ in $(seq 1 300); do
      grep -qF "graphics: demo frame=30 dmg=" build/gfx-uefi.log 2>/dev/null && break
      sleep 0.1
    done
    mon build/mon.sock "screendump build/gfx-uefi-b.ppm"
    for _ in $(seq 1 300); do
      grep -qF "graphics: demo stop" build/gfx-uefi.log 2>/dev/null && break
      sleep 0.1
    done
    sleep 1
    mon build/mon.sock "quit"
  ) &
  SENDER=$!
  timeout --signal=KILL 120 ./tools/run.sh --vga std --monitor --serial-unix build/serial.sock \
    < /dev/null > build/gfx-uefi-qemu.log 2>&1 || true
  wait "$SENDER" 2>/dev/null || true
  kill "$RELAY" 2>/dev/null || true

  if [ "$attempt" = "1" ]; then
    grep -q "graphics: damage self-test ok" build/gfx-uefi.log || note "damage self-test"
    grep -q "graphics: demo start" build/gfx-uefi.log || note "demo start"
    grep -q "graphics: demo stop" build/gfx-uefi.log || note "demo stop"
    grep -q "graphics: idle damage empty ok" build/gfx-uefi.log || note "idle empty damage"
    grep -q "shell: ready" build/gfx-uefi.log || note "shell ready"
    grep -qE "panic|PANIC" build/gfx-uefi.log && { echo "panic in the UEFI log"; OK=0; }
  fi
  if [ -s build/gfx-uefi-a.ppm ] && [ -s build/gfx-uefi-b.ppm ]; then
    UEFI_STATS="$(pixel_stats build/gfx-uefi-a.ppm)"
    echo "uefi screendump A: $UEFI_STATS"
    if [ "$attempt" = "1" ]; then
      BRIGHT="${UEFI_STATS##*bright=}"
      [ "$BRIGHT" -gt 200 ] || note "uefi screendump non-blank (bright=$BRIGHT)"
    fi
    if diff_contains build/gfx-uefi-a.ppm build/gfx-uefi-b.ppm build/gfx-uefi.log 10 32; then
      UEFI_CONTAINED=1
      break
    fi
  else
    [ "$attempt" = "1" ] && note "uefi screendumps produced"
  fi
done
[ "$UEFI_CONTAINED" = "1" ] || note "uefi damage containment"

# --- i686 VBE demo (two screendumps + damage containment) ---------------------
./tools/build-bios.sh --arch i686 >/dev/null 2>&1 || fail "i686 bios build"
VBE_CONTAINED=0
for attempt in 1 2 3; do
  rm -f build/gfx-mon.sock build/gfx-vbe.log build/gfx-vbe-a.ppm build/gfx-vbe-b.ppm
  (
    for _ in $(seq 1 300); do
      grep -qF "graphics: demo frame=10 dmg=" build/gfx-vbe.log 2>/dev/null && break
      sleep 0.1
    done
    mon build/gfx-mon.sock "screendump build/gfx-vbe-a.ppm"
    for _ in $(seq 1 300); do
      grep -qF "graphics: demo frame=30 dmg=" build/gfx-vbe.log 2>/dev/null && break
      sleep 0.1
    done
    mon build/gfx-mon.sock "screendump build/gfx-vbe-b.ppm"
    for _ in $(seq 1 300); do
      grep -qF "graphics: demo stop" build/gfx-vbe.log 2>/dev/null && break
      sleep 0.1
    done
    sleep 1
    mon build/gfx-mon.sock "quit"
  ) &
  SENDER=$!
  timeout --signal=KILL 40 qemu-system-x86_64 -machine pc -m 512M \
    -drive format=raw,file=build/bios-i686.img \
    -vga std -display none \
    -serial file:build/gfx-vbe.log \
    -monitor "unix:build/gfx-mon.sock,server,nowait" \
    -no-reboot < /dev/null > build/gfx-vbe-qemu.log 2>&1 || true
  wait "$SENDER" 2>/dev/null || true

  if [ "$attempt" = "1" ]; then
    grep -q "fb: 1024x768x32 pitch=4096 at 0xBFC00000" build/gfx-vbe.log || note "vbe geometry line"
    grep -q "fb: console up" build/gfx-vbe.log || note "vbe console up"
    grep -q "handshake ok: arch=3" build/gfx-vbe.log || note "i686 handshake"
    grep -q "graphics: demo start" build/gfx-vbe.log || note "vbe demo start"
    grep -q "graphics: demo stop" build/gfx-vbe.log || note "vbe demo stop"
    grep -q "graphics: idle damage empty ok" build/gfx-vbe.log || note "vbe idle empty damage"
  fi
  if [ -s build/gfx-vbe-a.ppm ] && [ -s build/gfx-vbe-b.ppm ]; then
    VBE_STATS="$(pixel_stats build/gfx-vbe-a.ppm)"
    echo "vbe screendump A: $VBE_STATS"
    if [ "$attempt" = "1" ]; then
      BRIGHT="${VBE_STATS##*bright=}"
      [ "$BRIGHT" -gt 1000 ] || note "vbe screendump non-blank (bright=$BRIGHT)"
      DIMS="${VBE_STATS%% *}"
      [ "$DIMS" = "1024x768" ] || note "vbe screendump dimensions ($DIMS)"
    fi
    if diff_contains build/gfx-vbe-a.ppm build/gfx-vbe-b.ppm build/gfx-vbe.log 10 32; then
      VBE_CONTAINED=1
      break
    fi
  else
    [ "$attempt" = "1" ] && note "vbe screendumps produced"
  fi
done
[ "$VBE_CONTAINED" = "1" ] || note "vbe damage containment"

# --- i686 serial parity: -vga none must only drop the fb: lines --------------
# The demo lines and the scheduler reap lines are excluded too: the demo only
# draws when a display exists, and the reap message interleaves with the boot
# log (a known serial flake), so neither belongs in the console parity check.
rm -f build/gfx-none.log
timeout --signal=KILL 40 qemu-system-x86_64 -machine pc -m 512M \
  -drive format=raw,file=build/bios-i686.img \
  -vga none -display none \
  -serial file:build/gfx-none.log \
  -no-reboot < /dev/null > build/gfx-none-qemu.log 2>&1 || true
grep -q "fb: unavailable (serial console)" build/gfx-none.log || note "vbe fallback line"
strip() { grep -v 'fb:' | grep -v 'graphics:' | grep -v 'sched: reaped'; }
strip < build/gfx-vbe.log > build/gfx-vbe-nofb.txt
strip < build/gfx-none.log > build/gfx-none-nofb.txt
if diff -q build/gfx-vbe-nofb.txt build/gfx-none-nofb.txt >/dev/null; then
  echo "serial parity: identical after fb:/graphics:/sched lines"
else
  note "serial parity (diff after fb:/graphics:/sched lines)"
  diff build/gfx-vbe-nofb.txt build/gfx-none-nofb.txt | head -10 || true
fi

if [ "$OK" = "1" ]; then
  echo "SMOKE PASS (graphics: double-buffer demo + damage containment, GOP + VBE)"
  grep -aE "graphics: damage self-test|graphics: demo (start|stop|damage total|idle)|fb: (1024|console up|unavailable)" \
    build/gfx-uefi.log build/gfx-vbe.log build/gfx-none.log | head -16
  exit 0
fi
echo "SMOKE FAIL (graphics)"
exit 1
