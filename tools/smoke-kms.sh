#!/usr/bin/env bash
# M13-4 KMS smoke: the dumb-buffer pool and the ADDFB/SETCRTC/PAGE_FLIP
# contract on the x86_64 GOP console.
#   boot with -vga std, assert the event-ring self-test, the addfb/setcrtc
#   markers, exactly FLIPS flip-complete events with seq 1..N, the negative
#   SETCRTC geometry-mismatch result, and the cleanup marker (dumb pool empty
#   + frame-allocator accounting back to the baseline).
#   two screendumps at different flip events must differ, and the differing
#   pixels must lie inside the framebuffer (the page-flip damage is the whole
#   frame).
# Timing is deterministic: screendumps are taken at the page_flip markers, and
# the demo freezes the screen for a full period after each flip.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p build

fail() { echo "SMOKE FAIL (kms): $*"; exit 1; }

# CONFIG_GRAPHICS is enabled by the rescue profile (M12-6 / M13-1).
python3 tools/kconfig.py --profile rescue >/dev/null || fail "config"
./tools/build.sh >/dev/null 2>&1 || fail "x86_64 build"

# A and B differ; the diff bounding box must lie inside the framebuffer (the
# addfb geometry), which is the whole-frame damage a page flip presents.
diff_inside() { # a.ppm b.ppm w h
  python3 - "$1" "$2" "$3" "$4" <<'PY'
import sys
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
fbw, fbh = int(sys.argv[3]), int(sys.argv[4])
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
    print("diff: images identical"); sys.exit(1)
dx = maxx - minx + 1
dy = maxy - miny + 1
inside = minx >= 0 and miny >= 0 and minx + dx <= fbw and miny + dy <= fbh
print(f"diff: {n} pixels bbox={minx},{miny},{dx},{dy} fb={fbw}x{fbh} "
      f"{'inside' if inside else 'OUTSIDE'}")
sys.exit(0 if inside else 1)
PY
}

mon() { # socket command
  printf '%s\n' "$2" | timeout 10 socat -t 1 - UNIX-CONNECT:"$1" >/dev/null 2>&1
}

OK=1
note() { echo "missing: $1"; OK=0; }

DIFF_OK=0
for attempt in 1 2 3; do
  rm -f build/mon.sock build/kms-serial.sock build/kms.log build/kms-a.ppm build/kms-b.ppm
  socat -u UNIX-LISTEN:build/kms-serial.sock,unlink-early OPEN:build/kms.log,creat,trunc &
  RELAY=$!
  (
    for _ in $(seq 1 300); do
      [ -S build/mon.sock ] && grep -qF "gfx: page_flip fb=2 event=1" build/kms.log 2>/dev/null && break
      sleep 0.1
    done
    mon build/mon.sock "screendump build/kms-a.ppm"
    for _ in $(seq 1 300); do
      grep -qF "gfx: page_flip fb=1 event=8" build/kms.log 2>/dev/null && break
      sleep 0.1
    done
    mon build/mon.sock "screendump build/kms-b.ppm"
    for _ in $(seq 1 300); do
      grep -qF "gfx: cleanup ok" build/kms.log 2>/dev/null && break
      sleep 0.1
    done
    sleep 1
    mon build/mon.sock "quit"
  ) &
  SENDER=$!
  timeout --signal=KILL 120 ./tools/run.sh --vga std --monitor --serial-unix build/kms-serial.sock \
    < /dev/null > build/kms-qemu.log 2>&1 || true
  wait "$SENDER" 2>/dev/null || true
  kill "$RELAY" 2>/dev/null || true

  if [ "$attempt" = "1" ]; then
    grep -q "gfx: event ring self-test ok" build/kms.log || note "event ring self-test"
    grep -q "gfx: addfb id=1 w=" build/kms.log || note "addfb id=1"
    grep -q "gfx: addfb id=2 w=" build/kms.log || note "addfb id=2"
    grep -q "gfx: setcrtc fb=1" build/kms.log || note "setcrtc fb=1"
    grep -q "gfx: page_flip fb=2 event=1" build/kms.log || note "page_flip event=1"
    grep -q "gfx: page_flip fb=1 event=8" build/kms.log || note "page_flip event=8"
    grep -q "gfx: flip loop ok frames=8" build/kms.log || note "flip loop ok"
    grep -q "gfx: setcrtc mismatch ok" build/kms.log || note "setcrtc mismatch"
    grep -q "gfx: cleanup ok" build/kms.log || note "cleanup ok"
    [ "$(grep -ac 'gfx: page_flip fb=' build/kms.log)" = "8" ] || note "flip event count"
    grep -qE "panic|PANIC" build/kms.log && { echo "panic in the kms log"; OK=0; }
  fi
  W=$(grep -a "gfx: addfb id=1 w=" build/kms.log | head -1 | sed -n 's/.* w=\([0-9]*\).*/\1/p')
  H=$(grep -a "gfx: addfb id=1 w=" build/kms.log | head -1 | sed -n 's/.* h=\([0-9]*\).*/\1/p')
  if [ -s build/kms-a.ppm ] && [ -s build/kms-b.ppm ] && [ -n "$W" ] && [ -n "$H" ]; then
    if diff_inside build/kms-a.ppm build/kms-b.ppm "$W" "$H"; then
      DIFF_OK=1
      break
    fi
  else
    [ "$attempt" = "1" ] && note "screendumps produced"
  fi
done
[ "$DIFF_OK" = "1" ] || note "flip screendump diff inside framebuffer"

if [ "$OK" = "1" ]; then
  echo "SMOKE PASS (kms: dumb buffers + ADDFB/SETCRTC/PAGE_FLIP + events + cleanup)"
  grep -aE "gfx: (event ring self-test|addfb|setcrtc|page_flip|flip loop ok|setcrtc mismatch|cleanup)" build/kms.log | head -20
  exit 0
fi
echo "SMOKE FAIL (kms)"
tail -25 build/kms.log 2>/dev/null || true
exit 1
